# recursum

Rust script to hash many files, quickly.

There are 3 modes of operation.

1. The original, and the root of the crate's name, is recursively checking every (non-symlink) file in a single given directory tree.
2. Hash any number of files given as arguments.
3. Take a list of files from stdin and hash each of them.

Parallelises file discovery (in usage #1) and hashing.
[Default hasher](https://mollyrocket.com/meowhash) is not cryptographically secure.

By default, `{path}\t{hex_digest}` is printed to stdout.
This is reversed compared to most hashing utilities (`md5sum`, `sha1sum` etc.) with the intention of making it easier to sort deterministically by file name, and because tabs (disallowed by many file system interfaces) are more reliable to split on than double spaces (an easy typo in file names).
However, the `--compatible` switch exists to print `{hex_digest}  {path}`.

Ongoing progress information, and a final time and rate, are printed to stderr.

Note that most hashers, particularly fast non-crypto hashes, will be faster than slower storage media like disks, so the gains from using many hashing threads may saturate quickly.

Contributions welcome.

## Installation

With `cargo` installed (get it with [rustup](https://rustup.rs/)):

```sh
cargo install recursum
```

## Usage

```
recursum
Hash lots of files fast, in parallel.

USAGE:
    recursum [FLAGS] [OPTIONS] <input>...

FLAGS:
    -c, --compatible    "Compatible mode", which prints the hash first and changes the default separator to double-
                        space, as used by system utilities like md5sum
    -h, --help          Prints help information
    -q, --quiet         Do not show progress information
    -V, --version       Prints version information

OPTIONS:
    -d, --digest-length <digest-length>    Maximum length of output hash digests
    -s, --separator <separator>            Separator. Defaults to tab unless --compatible is given. Use "\t" for tab and
                                           "\0" for null (cannot be mixed with other characters)
    -t, --threads <threads>                Hashing threads
    -w, --walkers <walkers>                Directory-walking threads, if <input> is a directory

ARGS:
    <input>...    One or more file names, one directory name (every file recursively will be hashed, in depth first
                  order), or '-' for getting list of files from stdin (order is conserved)
```

Example:

```sh
fd --threads 1 --type file | recursum --threads 10 --digest 64 - > my_checksums.txt
```

This could be more efficient, and have better logging, than using `--exec` or `| xargs`.

Note that `--separator` does not understand escape sequences.
In order to pass e.g. a tab as the separator, use `recursum -s $(echo '\t') -`

## Operation

Broadly speaking, `recursum` uses >= 1 thread to populate a queue of files to hash; either

1. lazily recursively iterating through directories
  - the internal queue is bounded, applying backpressure to minimise RAM wastage
  - this queue is considerably larger than the number of hashing threads, so they should never be waiting for the queue to be populated
2. taking them as an argument list
3. eagerly reading from stdin
  - this prevents the pipe buffer from filling up and blocking the source, which may not handle such a block gracefully
  - the internal queue is unbounded, and so may become very large if files are piped in much faster than they can be hashed

Simulaneously, items are popped off this queue and executed using tokio's threaded scheduler.
There should be no context switches within each task; the tasks are processed in the same order that they are received.
The main thread fetches results (in the same order) and prints them to stdout.

## Alternatives

`find` (or `fd`) with `-exec` (`--exec`), e.g.

```sh
find . -type f -exec md5sum {} \;
```

`find` is single-threaded, and `-exec` flattens the list of found files, passing each as an additional argument to the hashing utility.
This can break if the number of files is large.
Additionally, many built-in hashing utilities are not multi-threaded; furthermore, the utility is not actually called until the file list has been populated.

There you can also pipe a list of arguments to `xargs`, which can parallelise with `-P` and restrict the number of arguments given with `-n`:

```sh
find . -type f -print0 | xargs -0 -P 8 -n 1 -I _ md5sum "_"
```

This spawns a new shell for every invocation, which could be problematic, and may not make as good use of the CPU as there can be no communication between processes.

Even better would be to use `parallel` in "xargs mode".
There will be some overhead to the CPU due to multiple executions of the checksum tool, and RAM due to the way parallel buffers its output.

```sh
find . -type f | parallel -X md5sum
```

These tools are far more mature than recursum, so they may work better for you.
