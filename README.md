# woxel – Voxel-based Game Engine in Rust

A voxel-based Minecraft-style game built with **Rust**, **wgpu**, and **WebAssembly**, featuring both web and native desktop support.

![screenshot](screenshot.png)

### Quick Start
Install [Rustup](https://rustup.rs/)

```bash
# Add WASM target
rustup target add wasm32-unknown-unknown

# Install Trunk (one-time)
cargo install trunk

# Serve optimized WASM build
trunk serve --release
```

**Open**: http://localhost:8080 or check `dist/` folder


## Controls

| Action | Key | Mouse |
|--------|-----|-------|
| Move Forward | W | - |
| Move Back | S | - |
| Strafe Left | A | - |
| Strafe Right | D | - |
| Move Up | Space | - |
| Move Down | Shift | - |
| Look Around | - | Mouse Movement |
| Place Block | - | Right Click |
| Remove Block | - | Left Click |
| Toggle FreeCam | C | - |
| Full Mesh Reload | R | - |

## Configuration

- **Chunk Size**: 32x32x32 blocks (configurable in `model/world/chunk.rs`)
- **Render Distance**: Configurable (but max. ~32x32x32 chunks in 32bit mode)
- **Terrain Generation**: Perlin/FBM noise-based (see `model/world/terrain.rs`)

### Adding Features

1. **New Block Type**: Edit `model/world/block.rs`
2. **Terrain Generation**: Modify `model/world/terrain.rs`
3. **Rendering Changes**: Update `view/render.rs` or shaders
4. **Game Logic**: Add to `controller/` modules

## Known Limitations
- Single-Threaded
- Saving/Loading: In-memory only (no persistence)
- Advanced Physics: Basic gravity & collision
- Sound: No audio system yet
- Mobile: Not optimized for touch controls

## Technology Stack

| Component | Technology |
|-----------|------------|
| Graphics | [wgpu](https://github.com/gfx-rs/wgpu) |
| Math | [glam](https://github.com/bitshifter/glam-rs) |
| GUI | [egui](https://github.com/emilk/egui) |
| Web Bundle | [Trunk](https://trunkrs.io/) |
| Desktop Window | [winit](https://github.com/rust-windowing/winit) |
| Noise | Custom Perlin/FBM implementation |
