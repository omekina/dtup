# Installing dependencies

We will need three main things:

- Rust toolchain (rustc, cargo, stdlib)
- Bun
- GNU Make

So install all of them by using the guides bellow.

## Rust

If you're a Rust developer yourself, you probably know this drill already and will choose your own way.
And if you are not, then let's make it easy for you by using the simplest approach.

Go ahead and **install rustup** by following the instructions on the website and then the instructions in the terminal: <https://rustup.rs/>

Once you have rustup, you should run:

```bash
rustup default stable
```

That will pull all the necessary things to compile a standard Rust project.
And if you followed all the instructions given during rustup's installation process, you should have Rust ready and waiting to be used.

## Bun

This is the other thing we'll need to install.
As the dtup viewer is a web application, it needs a bundler to bundle it's client files.

You can install Bun by following this guide: <https://bun.sh/>

After you installed Bun, just try running the following command in your terminal to check if we have it up and ready:

```bash
bun -v
```

If it prints a version (three dot-separated numbers), then you installed it correctly.

## GNU Make

If you are on Linux you most probably already have make installed.
Just to check, try running the following command:

```bash
make -v
```

It should print a nice banner with some information like the version, license, warranty, ...

If it doesn't and says that the command does not exist, you will need to install it.

### Linux

Please check your distribution's package manager reference to see the name of the package and install it.
Sadly, there are just too many Linux distributions to cover in this guide.

### Mac

Use homebrew to install make: <https://formulae.brew.sh/formula/make>

### Windows

Use GnuWin32&mdash;follow the guide on this website: <https://gnuwin32.sourceforge.net/packages/make.htm>

## Wrapping up

Please make sure you installed all of the three dependencies (Rust, Bun, Make) before moving to the next chapter.

Now... give yourself a big pat on the back.
You installed all the dependencies and everything _should_ be smooth sailing from here on.
