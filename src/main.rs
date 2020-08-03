use std;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use digest::{Digest, Output};
use indicatif::{HumanBytes, HumanDuration, ProgressBar, ProgressStyle};
use jwalk::{Parallelism, WalkDir};
use meowhash::MeowHasher;
use num_cpus;
use std::time::Instant;
use structopt::StructOpt;
use tokio;
use tokio::io::AsyncBufReadExt;
use tokio::runtime;
use tokio::stream::{Stream, StreamExt};
use tokio::sync::mpsc;

const READ_BUFFER_SIZE: usize = 8 * 1024; // BufReader default, may want to increase
const HASH_BUFFER_SIZE: usize = 1024;

const BUFFER_PPN: f64 = 3.0;

fn queue_length(n_jobs: usize) -> usize {
    (n_jobs as f64 * BUFFER_PPN).ceil() as usize
}

fn stdin_paths() -> mpsc::UnboundedReceiver<PathBuf> {
    let (sender, receiver) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let stdin = tokio::io::BufReader::new(tokio::io::stdin());
        let mut line_stream = stdin.lines();

        while let Some(path_result) = line_stream.next().await {
            sender.send(PathBuf::from(&path_result.unwrap())).unwrap();
        }
    });
    receiver
}

// adapted from https://stackoverflow.com/a/58825638/2700168
fn walk_paths(
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

struct ResultOutput {
    started: Instant,
    total_files: u64,
    total_bytes: u64,
    progress: Option<ProgressBar>,
}

impl ResultOutput {
    // fn default() -> Self {
    //     Self {
    //         started: Instant::now(),
    //         total_files: 0,
    //         total_bytes: 0,
    //         progress: None,
    //     }
    // }

    // fn with_progress(pbar: ProgressBar) -> Self {
    //     Self {
    //         started: Instant::now(),
    //         total_files: 0,
    //         total_bytes: 0,
    //         progress: Some(pbar),
    //     }
    // }

    fn with_default_progress() -> Self {
        let spinner_style = ProgressStyle::default_spinner()
            .template("{bytes} | {elapsed} | {bytes_per_sec} | {msg}");
        let spinner = ProgressBar::new_spinner().with_style(spinner_style);
        Self {
            started: Instant::now(),
            total_files: 0,
            total_bytes: 0,
            progress: Some(spinner),
        }
    }

    fn handle_output(&mut self, path: &Path, hash: &str, size: u64) {
        println!("{:?}\t{}", path, hash);
        if let Some(ref mut p) = self.progress {
            p.set_message(&format!("{} {:?}", HumanBytes(size), path));
            p.inc(size);
        }
        self.total_files += 1;
        self.total_bytes += size;
    }

    fn finish(&mut self) {
        if let Some(ref mut p) = self.progress {
            p.finish_and_clear();
        }
        let elapsed = Instant::now().duration_since(self.started);
        let rate = (self.total_bytes as f64 / elapsed.as_secs_f64()).floor() as u64;
        eprintln!(
            "{} files ({}) hashed in {} ({}/s)",
            self.total_files,
            HumanBytes(self.total_bytes),
            HumanDuration(elapsed),
            HumanBytes(rate),
        );
    }
}

async fn hash_from_stream<S: Stream<Item = PathBuf> + Unpin>(
    mut path_stream: S,
    truncate_to: Option<usize>,
    n_jobs: usize,
) {
    let mut output = ResultOutput::with_default_progress();

    let mut fut_queue = VecDeque::with_capacity(n_jobs);
    let mut is_finished = false;

    let queue_len = queue_length(n_jobs);

    // make sure there are n_jobs running before looking at results
    for _ in 0..queue_len {
        if let Some(path) = path_stream.next().await {
            // todo: factor out
            fut_queue.push_back(tokio::spawn(async move {
                let (hash, size) = hash_file(path.as_path(), MeowHasher::new(), truncate_to);
                (path, hash, size)
            }));
        } else {
            // there were fewer than n_jobs to begin with
            is_finished = true;
            break;
        }
    }

    if !is_finished {
        // pop the first job off the queue when completed, spawn another and append to queue
        while let Some(path) = path_stream.next().await {
            let result = fut_queue.pop_front().unwrap().await.unwrap();
            output.handle_output(result.0.as_path(), result.1.as_str(), result.2 as u64);
            fut_queue.push_back(tokio::spawn(async move {
                let (hash, size) = hash_file(path.as_path(), MeowHasher::new(), truncate_to);
                (path, hash, size)
            }));
        }
    }

    for fut in fut_queue.into_iter() {
        let result = fut.await.unwrap();
        output.handle_output(result.0.as_path(), result.1.as_str(), result.2 as u64);
    }
    output.finish();
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
    let mut buf_reader = std::io::BufReader::with_capacity(READ_BUFFER_SIZE, reader);
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
#[structopt(name = "recursum", about = "Hash lots of files fast, in parallel.")]
struct Opt {
    /// File name, directory name (every file recursively will be hashed, in depth first order), or '-' for getting list of files from stdin (order is conserved)
    #[structopt()]
    input: OsString,
    /// Directory-walking threads, if input is a directory
    #[structopt(short = "w", long = "walkers")]
    walkers: Option<usize>,
    /// Hashing threads
    #[structopt(short = "t", long = "threads")]
    threads: Option<usize>,
    /// Maximum length of output hash digests
    #[structopt(short = "d", long = "digest")]
    digest_length: Option<usize>,
}

enum InputConfig {
    Directory((usize, PathBuf, usize)),
    Stdin(usize),
}

impl InputConfig {
    async fn hash(&self, truncate_to: Option<usize>) {
        match self {
            Self::Directory((n_jobs, root, walkers)) => {
                let stream = walk_paths(
                    root.clone(),
                    queue_length(*n_jobs),
                    Parallelism::RayonNewPool(*walkers),
                );
                hash_from_stream(stream, truncate_to, *n_jobs).await;
            }
            Self::Stdin(n_jobs) => {
                let stream = stdin_paths();
                hash_from_stream(stream, truncate_to, *n_jobs).await;
            }
        }
    }
}

fn handle_single_file(path: &Path, truncate: Option<usize>) {
    let started = Instant::now();
    let (digest, size) = hash_file(path, MeowHasher::new(), truncate);
    println!("{:?}\t{}", path, digest);
    let elapsed = Instant::now().duration_since(started);
    let rate = (size as f64 / elapsed.as_secs_f64()).floor() as u64;
    eprintln!(
        "{} files ({}) hashed in {} ({}/s)",
        1,
        HumanBytes(size as u64),
        HumanDuration(elapsed),
        HumanBytes(rate),
    );
}

fn main() {
    let opt = Opt::from_args();
    let threads = or_num_cpus(opt.threads);

    let input;
    if opt.input == "-" {
        input = InputConfig::Stdin(threads);
    } else {
        let path = PathBuf::from(opt.input);
        if path.is_dir() {
            let walkers = or_num_cpus(opt.walkers);
            input = InputConfig::Directory((threads, path, walkers))
        } else if path.is_file() {
            handle_single_file(&path, opt.digest_length);
            return;
        } else {
            panic!("Given input is not a directory, file, or - for stdin");
        }
    }

    let mut rt = runtime::Builder::new()
        .enable_all()
        .threaded_scheduler()
        .core_threads(threads)
        .build()
        .unwrap();

    rt.block_on(input.hash(opt.digest_length));
}
