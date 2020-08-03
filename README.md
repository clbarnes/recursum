# recursum

Rust script to hash many files, quickly.

There are 3 modes of operation.

1. The original, and the root of the crate's name, is recursively checking every (non-symlink) file in a given directory tree.
2. Hash a single given file.
3. Take a list of files from stdin and hash each of them.

Parallelises file discovery (in usage #1) and hashing (#1 and #3).
[Default hasher](https://mollyrocket.com/meowhash) is not cryptographically secure.

`"{path}"\t{hex_digest}` printed to stdout; progress information on stderr.

Contributions welcome.

## Usage

```
recursum
Hash lots of files fast, in parallel.

USAGE:
    recursum [OPTIONS] <input>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -d, --digest <digest-length>    Maximum length of output hash digests
    -t, --threads <threads>         Hashing threads
    -w, --walkers <walkers>         Directory-walking threads, if input is a directory

ARGS:
    <input>    File name, directory name (every file recursively will be hashed, in depth first order), or '-' for
               getting list of files from stdin (order is conserved)
```

Example:

```sh
fd --threads 1 --type file | recursum --threads 10 --digest 64 - > my_checksums.txt
```

This should be much more efficient, and with better logging, than using `--exec` or `| xargs`.

## Notes

In mode #1, there is backpressure which minimises wasted RAM (files are only discovered shortly before they're needed).
In mode #3, this is not the case because by default, rust cannot use a full pipe buffer as backpressure.
If the directory tree is very (*very*) large, and the files are listed faster than they're hashed (likely), there may be quite some RAM wastage.
