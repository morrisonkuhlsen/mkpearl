# Vendored crates

## winit 0.30.13

An unmodified copy of the published crate (Apache-2.0, see `winit/LICENSE`) but for these changes,
because winit 0.30's Wayland backend has no drag and drop and its maintainers won't add it to 0.30
(rust-windowing/winit#4810). eframe 0.37 needs winit 0.30, and reaching winit's connection from Pearl
would take `unsafe`.

- New: `src/platform_impl/linux/wayland/seat/data_device.rs` and `seat/uri_list.rs`.
- Changed: `seat/mod.rs` and `state.rs` in `src/platform_impl/linux/wayland/`.
- Removed: the examples, the tests and their sections in `Cargo.toml`, and `docs/` with its pictures, none of which build Pearl.

Delete this folder and the `[patch.crates-io]` in Pearl's `Cargo.toml` once eframe is on a winit with
Wayland drag and drop.
