use std::collections::VecDeque;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::path::{Path, PathBuf};

use digest::{Digest, Output};
use indicatif::{HumanBytes, HumanDuration, ProgressBar, ProgressStyle};
use jwalk::{Parallelism, WalkDir};
use meowhash::MeowHasher;
use num_cpus;
use pathdiff::diff_paths;
use std::time::Instant;
use structopt::StructOpt;
use tokio::runtime;
use tokio::sync::mpsc;

const READ_BUFFER_SIZE: usize = 8 * 1024; // BufReader default, may want to increase
const HASH_BUFFER_SIZE: usize = 1024;

// adapted from https://stackoverflow.com/a/58825638/2700168
fn path_queue(
    root: PathBuf,
    queue_len: usize,
    parallelism: Parallelism,
) -> mpsc::Receiver<PathBuf> {
    let (mut sender, receiver) = mpsc::channel(queue_len);

    tokio::spawn(async move {
        for entry in WalkDir::new(root)
            .parallelism(parallelism)
            .follow_links(false)
            .sort(true)
        {
            let e = entry.unwrap();
            if e.file_type().is_file() {
                sender.send(e.path()).await.unwrap();
            }
        }
    });

    receiver
}

async fn hash_all(root: PathBuf, n_walkers: usize, queue_len: usize, truncate_to: Option<usize>) {
    let mut receiver = path_queue(
        root.clone(),
        queue_len,
        Parallelism::RayonNewPool(n_walkers),
    );
    let mut fut_queue = VecDeque::with_capacity(queue_len);
    let mut is_finished = false;

    let spinner_style =
        ProgressStyle::default_spinner().template("{bytes} | {elapsed} | {bytes_per_sec} | {msg}");
    let spinner = ProgressBar::new_spinner().with_style(spinner_style);
    let mut total_files: usize = 0;
    let mut total_bytes: usize = 0;
    let started = Instant::now();

    for _ in 0..queue_len {
        if let Some(path) = receiver.recv().await {
            fut_queue.push_back(tokio::spawn(async move {
                let (hash, size) = hash_file(path.as_path(), MeowHasher::new(), truncate_to);
                (path, hash, size)
            }));
        } else {
            is_finished = false;
            break;
        }
    }

    while !is_finished {
        if let Some(path) = receiver.recv().await {
            let output = fut_queue.pop_front().unwrap().await.unwrap();
            spinner.set_message(&format!("{} {:?}", HumanBytes(output.2 as u64), output.0));
            spinner.inc(output.2 as u64);
            total_bytes += output.2;
            total_files += 1;
            println!("{:?}\t{}", diff_paths(output.0, &root).unwrap(), output.1);
            fut_queue.push_back(tokio::spawn(async move {
                let (hash, size) = hash_file(path.as_path(), MeowHasher::new(), truncate_to);
                (path, hash, size)
            }));
        } else {
            is_finished = true;
        }
    }

    for fut in fut_queue.into_iter() {
        let output = fut.await.unwrap();
        spinner.set_message(&format!("{} {:?}", HumanBytes(output.2 as u64), output.0));
        spinner.inc(output.2 as u64);
        total_bytes += output.2;
        total_files += 1;
        println!("{:?}\t{}", diff_paths(output.0, &root).unwrap(), output.1);
    }
    spinner.finish_and_clear();
    let elapsed = Instant::now().duration_since(started);
    let rate = (total_bytes as f64 / elapsed.as_secs_f64()).floor() as u64;
    eprintln!(
        "{} files ({}) hashed in {} ({}/s) from {:?}",
        total_files,
        HumanBytes(total_bytes as u64),
        HumanDuration(elapsed),
        HumanBytes(rate),
        root
    );
}

fn hash_file<D: Digest>(fpath: &Path, hasher: D, truncate: Option<usize>) -> (String, usize) {
    let file = File::open(fpath).unwrap();
    let (hash, size) = hash_reader(file, hasher);
    let mut digest = hex::encode(hash);
    if let Some(t) = truncate {
        digest.truncate(t);
    }
    (digest, size)
}

// adapted from https://rust-lang-nursery.github.io/rust-cookbook/cryptography/hashing.html#calculate-the-sha-256-digest-of-a-file
fn hash_reader<R: Read, D: Digest>(reader: R, mut hasher: D) -> (Output<D>, usize) {
    let mut buf_reader = BufReader::with_capacity(READ_BUFFER_SIZE, reader);
    let mut size = 0;

    let mut buf = [0; HASH_BUFFER_SIZE];
    loop {
        let count = buf_reader.read(&mut buf).expect("could not read file");
        if count == 0 {
            break;
        }
        hasher.update(&buf[..count]);
        size += count;
    }
    (hasher.finalize(), size)
}

fn or_num_cpus(opt: Option<usize>) -> usize {
    opt.unwrap_or_else(num_cpus::get)
}

#[derive(Debug, StructOpt)]
#[structopt(name = "recursum", about = "Recursively hash all files in a directory, fast.")]
struct Opt {
    /// Input file
    #[structopt(parse(from_os_str))]
    input: PathBuf,
    /// Directory-walking threads
    #[structopt(short = "w", long = "walkers")]
    walkers: Option<usize>,
    /// Hashing threads
    #[structopt(short = "t", long = "threads")]
    threads: Option<usize>,
    /// Buffer factor (how large inter-thread buffers should be, as a proportion of threads)
    #[structopt(short = "b", long = "buffer", default_value = "3")]
    buffer: f64,
    /// Maximum length of output hash digests
    #[structopt(short = "d", long = "digest")]
    digest_length: Option<usize>,
}

fn main() {
    let opt = Opt::from_args();
    let threads = or_num_cpus(opt.threads);
    let walkers: usize = or_num_cpus(opt.walkers);

    let queue_len = (opt.buffer * threads as f64).ceil() as usize;
    let mut rt = runtime::Builder::new()
        .enable_all()
        .threaded_scheduler()
        .core_threads(threads)
        .build()
        .unwrap();
    rt.block_on(hash_all(opt.input, walkers, queue_len, opt.digest_length));
}
