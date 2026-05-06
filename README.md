# tool-study-session
Tool I use when studying

## Build on Linux

```bash
./scripts/build-linux.sh
```

The executable will be created at:

```bash
dist/linux/tool-study-session
```

Run it directly:

```bash
./dist/linux/tool-study-session
```

## Install on Linux

```bash
./scripts/install-linux.sh
```

This installs the executable to `~/.local/bin/tool-study-session` and adds a
desktop launcher at `~/.local/share/applications/tool-study-session.desktop`.

Todo data is stored in `~/.local/share/tool-study-session/todos.json`, or under
`$XDG_DATA_HOME/tool-study-session/todos.json` when `XDG_DATA_HOME` is set.

To run the app do:
~/.local/bin/tool-study-session
