# Sidecar

Sidecar consists of a Roblox Studio plugin and a server binary. The intended use case is to export a collection of scripts as files, and then watch for file changes and update the script associated to each file in real time back in Roblox Studio.  The idea is that working with deeply nested folders containing the entire source code can be cumbersome when code may be quite scattered.

### How to use
1. Install the Roblox Studio plugin
2. Download the latest server binary or build it from source
3. Start the server from a terminal in the current directory you want to use as a workspace
4. Export files from the plugin
5. Use your favourite editor to modify the files and instantly test in Roblox Studio

### How to build server from source (for plugin development)
```
git clone https://github.com/AxisAngles/sidecar
cd sidecar/server
cargo build --release
```
The compiled binary will be located at `sidecar/server/target/release/sidecar.exe`
