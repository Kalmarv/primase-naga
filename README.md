# primase-naga

[![crates.io: primase-naga](https://img.shields.io/crates/v/primase-naga?label=primase-naga)](https://crates.io/crates/primase-naga) [![docs.rs](https://img.shields.io/docsrs/primase-naga)](https://docs.rs/primase-naga) [![CI](https://img.shields.io/github/actions/workflow/status/Kalmarv/primase-naga/ci.yml?branch=main&label=CI)](https://github.com/Kalmarv/primase-naga/actions/workflows/ci.yml)

A fork of [naga](https://github.com/gfx-rs/wgpu/tree/trunk/naga) 30.0.1, the shader translator and validator from the wgpu project, with additional shader translation support. It is used by [iridia](https://github.com/Kalmarv/iridia), which lifts compiled Direct3D shaders into naga modules that need these additions.

## Features

- Writes ray queries in the WGSL backend (`rayQueryInitialize`, `rayQueryProceed`, the intersection and hit-vertex-position getters) and enables `wgpu_ray_query` and `wgpu_ray_query_vertex_return` as needed
- Adds an optional `result` to `Statement::ImageAtomic` that holds the texel's previous value, written by the SPIR-V, HLSL, MSL and GLSL backends
- Accepts `AtomicFunction::Exchange` in `ImageAtomic` on 32-bit integer images, written by the SPIR-V, HLSL and GLSL backends
- Preserves producer-local `NoContraction` on scalar/vector addition, subtraction and multiplication, and floating-point dot products, through SPIR-V and supported GLSL/HLSL output
- Reads WGSL, SPIR-V and GLSL (Vulkan 440 to 460)
- Writes WGSL, SPIR-V, HLSL, MSL, GLSL (core 330+ and ES 300+) and Graphviz DOT
- Validates modules against a configurable set of capabilities
- Keeps the library name `naga`; exhaustive `Function` literals must initialize the added `expression_controls` map
- Returns an unsupported-feature error from backends that cannot express an addition (an image atomic result in WGSL, image exchange in WGSL and MSL)
- Builds as `no_std` with `alloc` and forbids unsafe code

## Getting Started

### Requirements

Rust 1.87 or newer.

### Installation

primase-naga is published on crates.io. Rename it to `naga` in `Cargo.toml` and enable the front and back ends you need. No features are enabled by default.

```toml
[dependencies]
naga = { package = "primase-naga", version = "30.0.1", features = ["wgsl-in", "wgsl-out"] }
```

The front-end features are `wgsl-in`, `spv-in` and `glsl-in`. The back-end features are `wgsl-out`, `spv-out`, `hlsl-out`, `msl-out`, `glsl-out` and `dot-out`. `hlsl-out-if-target-windows` and `msl-out-if-target-apple` enable those back ends only on the matching platform.

### Usage

This parses a WGSL compute shader that uses a ray query, validates it and writes it back out as WGSL. Stock naga 30.0.1 panics in its WGSL backend on this module.

```rust
use naga::back::wgsl::WriterFlags;
use naga::valid::{Capabilities, ValidationFlags, Validator};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
enable wgpu_ray_query;

@group(0) @binding(0) var acc: acceleration_structure;

@compute @workgroup_size(1)
fn main() {
    var rq: ray_query;
    rayQueryInitialize(&rq, acc, RayDesc(0u, 255u, 0.1, 10.0, vec3<f32>(0.0), vec3<f32>(0.0, 1.0, 0.0)));
}
"#;

    let module = naga::front::wgsl::parse_str(source)?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::RAY_QUERY).validate(&module)?;
    let wgsl = naga::back::wgsl::write_string(&module, &info, WriterFlags::empty())?;

    // Starts with `enable wgpu_ray_query;` and calls `rayQueryInitialize((&rq), acc, RayDesc(...))`.
    println!("{wgsl}");
    Ok(())
}
```

Arithmetic controls are attached to `Function::expression_controls` and apply only to the producing operation. They survive cloning, serialization, specialization and compaction. Controlled expressions are runtime values and cannot initialize constant-or-override local variables; emit the expression and store it instead. Unsupported operations and output profiles return errors, including WGSL/MSL output and HLSL before Shader Model 6. GLSL requires desktop 400+ or ES 320+, and controlled integer arithmetic requires 32-bit integers. HLSL 16-bit controls require Shader Model 6.2; 64-bit floating-point dot controls are unsupported.

For unary scalar-f32 Math(Exp2), ArithmeticControl::Exp2F32NoRefactoringV1 withholds fast-math permissions on that producer. SPIR-V import/export uses per-result FPFastMathMode None, requiring SPIR-V 1.2+, SPV_KHR_float_controls2, and the FloatControls2 capability (including permission in an explicit capability allowlist). HLSL uses a scalar [noinline] helper containing precise Exp2, with its argument evaluated outside the helper. Use Shader Model 6+ and a qualified DXC compiler; a shader-model setting does not select or authenticate the compiler. GLSL, MSL and WGSL reject this control under their existing function-selection policies. Vectors, f16/f64 and other operations are unsupported. This control is mutually exclusive with NoContraction on the same producer. It does not recursively constrain arguments or consumers, guarantee numerical accuracy or denormal behavior, or establish device support.

A SPIR-V module that declares FloatControls2 is written in the explicit SPV_KHR_float_controls2 form. Each entry point sets FPFastMathDefault to NotNaN|NotInf|NSZ|AllowRecip|AllowContract|AllowReassoc|AllowTransform (the Vulkan defaults) for every float width. NoContraction on a float result is written as FPFastMathMode NotNaN|NotInf|NSZ, and NoContraction on an integer result is omitted because the wrapping operation is exact. Modules that do not declare FloatControls2 keep the NoContraction decoration. SPIR-V import maps both forms to the same controls and rejects any other FPFastMathDefault or FPFastMathMode mask.

The remaining API follows naga 30.0.1's API, which is documented at [docs.rs/naga/30.0.1](https://docs.rs/naga/30.0.1).

Run the library's unit tests with `cargo test --lib --all-features`. Upstream's snapshot and integration tests depend on the wgpu workspace and are not included. Changes from naga are listed in [CHANGELOG.md](CHANGELOG.md). Licensed under MIT or Apache-2.0, as naga is ([LICENSE.MIT](LICENSE.MIT), [LICENSE.APACHE](LICENSE.APACHE)).
