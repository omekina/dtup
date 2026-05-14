# Running dtq

In the [previous chapter](./building.md) you have prepared the executable dtq binary into the directory in which you develop your devicetrees.

Now go ahead and open a terminal in that directory.
Now you need to locate your devicetree entrypoint file&mdash;the file which you pass to the compiler as an argument.
Now you should run dtq on it.

```bash
./dtq <file>
```

Where `<file>` is the path to your entrypoint file (relative to the directory in which you opened your terminal just now).

After running that&mdash;dtq should jump in and start running.
If your devicetree is somehow incorrect, dtq will emit errors and refuse to print the processed tree.
If there are only some non-fatal warnings dtq will print them and then the compilation time and the tree.
If there are no problems, then dtq will only print out the compilation time and the tree.
