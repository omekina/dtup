# Building dtview

Locate the `dtup` directory which you created in the [download part](../installation/cloning_the_repo.md) of this book.
If you have all the [dependencies](../installation/installing_dependencies.md) installed, you should be able to build dtview simply by running a build through cargo.
Open a terminal in the `dtup/dtview` directory and run:

```bash
make
```

Now wait for the build to finish.
After the build finishes you should see a binary in `dtup/target/debug/dtview` (where `dtup` is the directory where we downloaded the dtup code to).

Now the simplest way to use this binary is to copy it into the directory in which you have your devicetree source files.
You can also install it system-wide, but that might not work for some operating systems, so if you're not sure&mdash;rather copy it into the same directory in which you develop your devicetrees.
