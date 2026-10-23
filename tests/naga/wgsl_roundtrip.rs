//! Tests that WGSL the WGSL back end writes is WGSL the front end reads back to the same program.

use naga::{
    back::wgsl::WriterFlags,
    valid::{Capabilities, ModuleInfo, ValidationFlags, Validator},
};

fn parse(source: &str) -> (naga::Module, ModuleInfo) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{}\n{source}", e.emit_to_string(source)));
    let info = Validator::new(
        ValidationFlags::all(),
        Capabilities::RAY_QUERY | Capabilities::RAY_HIT_VERTEX_POSITION,
    )
    .validate(&module)
    .expect("module should be valid");
    (module, info)
}

fn write(module: &naga::Module, info: &ModuleInfo) -> String {
    naga::back::wgsl::write_string(module, info, WriterFlags::empty()).expect("WGSL write failed")
}

/// Write `input`'s module as WGSL and read that back, twice over: the program is the same, so the text of the second and
/// third writes is. (The first read renames what the namer renames, such as `rq_1` to `rq_1_`; after that the names are
/// fixed.)
#[track_caller]
fn assert_roundtrips(input: &str) {
    let (module, info) = parse(input);
    let once = write(&module, &info);
    let (module, info) = parse(&once);
    let twice = write(&module, &info);
    let (module, info) = parse(&twice);
    let thrice = write(&module, &info);
    assert_eq!(twice, thrice);
}

#[test]
fn ray_queries() {
    for name in [
        "ray-query",
        "aliased-ray-query",
        "ray-query-no-init-tracking",
        "ray-query-vertex-return",
    ] {
        let path = format!("{}/tests/in/wgsl/{name}.wgsl", env!("CARGO_MANIFEST_DIR"));
        let input = std::fs::read_to_string(&path).unwrap();
        println!("{name}");
        assert_roundtrips(&input);
    }
}

/// The extensions a module needs are the ones it enables: a ray query needs `wgpu_ray_query`, not the pipeline's.
#[test]
fn ray_queries_enable_the_ray_query_extension() {
    let source = "enable wgpu_ray_query;\n@group(0) @binding(0) var acc: acceleration_structure;\n\
                  @compute @workgroup_size(1) fn main() {\n    var rq: ray_query;\n\
                  rayQueryInitialize(&rq, acc, RayDesc(0u, 255u, 0.1, 10.0, vec3<f32>(0.0), vec3<f32>(0.0, 1.0, 0.0)));\n}\n";
    let (module, info) = parse(source);
    let text = write(&module, &info);
    assert!(text.starts_with("enable wgpu_ray_query;\n"), "{text}");
    assert!(!text.contains("wgpu_ray_tracing_pipeline"), "{text}");
    assert!(!text.contains("struct RayDesc"), "{text}");
}

/// A variable named like a predeclared type of the extensions does not hide the type from the declarations after it.
#[test]
fn a_ray_query_variable_named_like_its_type_is_renamed() {
    let mut module = naga::front::wgsl::parse_str(
        "enable wgpu_ray_query;\n@compute @workgroup_size(1) fn main() {\n    var rq: ray_query;\n    var rq_2: ray_query;\n}\n",
    )
    .unwrap();
    for (_, function) in module.entry_points.iter_mut().map(|e| (0, &mut e.function)) {
        for (_, local) in function.local_variables.iter_mut() {
            local.name = Some("ray_query".into());
        }
    }
    let info = Validator::new(ValidationFlags::all(), Capabilities::RAY_QUERY)
        .validate(&module)
        .unwrap();
    let text = write(&module, &info);
    assert!(!text.contains("var ray_query:"), "{text}");
    // and the output reads back
    parse(&text);
}
