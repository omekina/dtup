# Building dtq

Locate the `dtup` directory which you created in the [download part](../installation/cloning_the_repo.md) of this book.
If you have all the [dependencies](../installation/installing_dependencies.md) installed, you should be able to build dtq simply by running a build through cargo.
Open a terminal in the previously mentioned `dtup` directory and run:

```bash
cargo build -p dtq --release
```

Now wait for the build to finish.
After the build finishes you should see a binary in `dtup/target/release/dtq` (where `dtup` is the directory in which we have the opened terminal).

Now the simplest way to use this binary is to copy it into the directory in which you have your devicetree source files.
You can also install it system-wide, but that might not work for some operating systems, so if you're not sure&mdash;rather copy it into the same directory in which you develop your devicetrees.
