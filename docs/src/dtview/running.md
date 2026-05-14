# Running dtview

In the [previous chapter](./building.md) you have prepared the executable dtview binary into the directory in which you develop your devicetrees.

Now go ahead and open a terminal in that directory.
Now you need to locate your devicetree entrypoint file&mdash;the file which you pass to the compiler as an argument.
Now you should run dtview on it.

```bash
./dtview <file>
```

Where `<file>` is the path to your entrypoint file (relative to the directory in which you opened your terminal just now).

## Viewing the tree

This command will start the dtview server and give your tree to it.
dtview will start listening on `http://localhost:8080`.
So as it's running, open a browser next to it and go to the following URL: <http://localhost:8080>.
You either should see your tree already, or you should see an error and will be told to check the console.
Don't worry&mdash;that's intentional.
If you see an error, just look at the terminal in which you have the dtview server running, it will show you everything.

## Live-updating

Now once you have it all set-up you can start modifying your devicetree source files.
Each time you save the file in your editor, you should see the tree in your browser get immediately updated.
If there is an error, the tree will disappear and you'll have to look at the console output to read it.
Note that errors might get moved to the browser as the project evolves.

## Stopping the server

If you wish to stop the server, jump into the terminal window and press `Ctrl+C` anytime.
The server will perform a clean-up and should end within a second.
