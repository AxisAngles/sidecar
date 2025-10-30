# Sidecar

Sidecar is a Roblox development tool that consists of a Roblox Studio plugin and a server binary. Export a selection of scripts as files, and have file changes sync back to Roblox Studio in real time.

### Why Sidecar?
The idea is that working with deeply nested folders containing the entire source code can be cumbersome when code may be quite scattered.  The explicit export and one-way sync model means you don't have to guess what the plugin is going to do to your project files.

### How to set up the workflow
1. Install the Roblox Studio plugin
2. Download the latest server binary or build it from source
3. Start the server from a terminal in the current directory you want to use as a workspace
4. Click the "connect" button on the plugin menu

### The workflow
1. Export files from the plugin
2. Use your favourite editor and instantly test in Roblox Studio

### How to build server from source (for plugin development)
```
git clone https://github.com/AxisAngles/sidecar
cd sidecar/server
cargo build --release
```
The compiled binary will be located at `sidecar/server/target/release/sidecar.exe`
