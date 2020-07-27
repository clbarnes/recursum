# recursum

Rust script to hash every file in a directory, quickly.

Parallelises file discovery and hashing.
[Default hasher](https://mollyrocket.com/meowhash) is not cryptographically secure.

`"{path}\t{hex_digest}"` printed to stdout; progress information on stderr.

Contributions welcome.

```
recursum 0.1.0
Recursively hash all files in a directory, fast.

USAGE:
    treesum [OPTIONS] <input>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -b, --buffer <buffer>           Buffer factor (how large inter-thread buffers should be, as a proportion of threads)
                                    [default: 3]
    -d, --digest <digest-length>    Maximum length of output hash digests
    -t, --threads <threads>         Hashing threads
    -w, --walkers <walkers>         Directory-walking threads

ARGS:
    <input>    Input file
```
