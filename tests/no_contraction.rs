use naga::{back, valid, ArithmeticControl, BinaryOperator, Expression, Module};
fn validate(m: &Module) -> Result<valid::ModuleInfo, naga::WithSpan<valid::ValidationError>> {
    valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all()).validate(m)
}
fn marked(body: &str) -> Module {
    let mut m = naga::front::wgsl::parse_str(&format!("override a:f32=1.; override b:f32=2.; @group(0) @binding(0) var<storage,read_write> out:f32; @compute @workgroup_size(1) fn main(){{{body}}}")).unwrap();
    let f = &mut m.entry_points[0].function;
    let h = f
        .expressions
        .iter()
        .find(|(_, e)| {
            matches!(
                e,
                Expression::Binary {
                    op: BinaryOperator::Multiply,
                    ..
                }
            )
        })
        .unwrap()
        .0;
    f.expression_controls
        .insert(h, ArithmeticControl::NoContraction);
    m
}
#[test]
fn controlled_initializers_stores_and_invalid_shapes() {
    for (label, body) in [
        ("direct", "var x:f32=a*b;out=x;"),
        ("transitive_binary_unary", "var x:f32=-(a*b)+b;out=x;"),
        ("composite", "var x:vec2<f32>=vec2<f32>(a*b,a);out=x.x;"),
        ("splat", "var x:vec2<f32>=vec2<f32>(a*b);out=x.y;"),
        ("select", "var x:f32=select(a*b,b,true);out=x;"),
    ] {
        let m = marked(body);
        assert!(m.entry_points[0]
            .function
            .local_variables
            .iter()
            .any(|(_, v)| v.init.is_some()));
        let error = validate(&m).unwrap_err();
        assert!(
            format!("{error:?}").contains("NonConstOrOverrideInitializer"),
            "{label}: {error:?}"
        );
        println!(
            "PASS {label}: controlled constant/override initializer explicitly rejected: {error:?}"
        );
        let mut unmarked = m.clone();
        unmarked.entry_points[0]
            .function
            .expression_controls
            .clear();
        let info = validate(&unmarked).unwrap();
        let (specialized, info) = back::pipeline_constants::process_overrides(
            &unmarked,
            &info,
            None,
            &Default::default(),
        )
        .unwrap();
        back::spv::write_vec(&specialized, &info, &Default::default(), None).unwrap();
        println!("PASS {label}: unmarked initializer specializes and emits");
    }
    for (label, body) in [
        ("explicit_store", "var x:f32; x=a*b;out=x;"),
        ("let_store", "let x=a*b;out=x;"),
    ] {
        let m = marked(body);
        let info = validate(&m).unwrap();
        let (specialized, info) =
            back::pipeline_constants::process_overrides(&m, &info, None, &Default::default())
                .unwrap();
        validate(&specialized).unwrap();
        let words = back::spv::write_vec(&specialized, &info, &Default::default(), None).unwrap();
        let mut cursor = 5;
        let mut controls = 0;
        while cursor < words.len() {
            let count = (words[cursor] >> 16) as usize;
            if words[cursor] & 65535 == 71 && words[cursor + 2] == 42 {
                controls += 1;
            }
            cursor += count;
        }
        assert_eq!(controls, 1);
        println!("PASS {label}: specialization and SPIR-V emission retain one producing control");
    }
    for (label, expr) in [
        ("divide", "out / vec4<f32>(2.0)"),
        ("fma", "fma(out,out,out)"),
        ("mixed_shape", "out * 2.0"),
        (
            "matrix",
            "mat2x2<f32>(out.xy,out.zw) * mat2x2<f32>(out.xy,out.zw)",
        ),
    ] {
        let mut m=naga::front::wgsl::parse_str(&format!("@group(0) @binding(0) var<storage,read_write> out:vec4<f32>; @compute @workgroup_size(1) fn main(){{let bad={expr};out=out;}}")).unwrap();
        let f = &mut m.entry_points[0].function;
        let h = *f
            .named_expressions
            .iter()
            .find(|(_, n)| n.as_str() == "bad")
            .unwrap()
            .0;
        f.expression_controls
            .insert(h, ArithmeticControl::NoContraction);
        let err = validate(&m).unwrap_err();
        assert!(format!("{err:?}").contains("InvalidArithmeticControl"));
        println!("PASS invalid controlled {label} rejected at IR validation");
    }
}

#[test]
fn controls_remap_without_retaining_dead_producers() {
    let mut module = naga::front::wgsl::parse_str("@group(0) @binding(0) var<storage,read_write> out:f32; @compute @workgroup_size(1) fn main(){let dead=out+1.0;let live=out*out;out=live;}").unwrap();
    let function = &mut module.entry_points[0].function;
    let old_live = *function
        .named_expressions
        .iter()
        .find(|(_, n)| n.as_str() == "live")
        .unwrap()
        .0;
    let producers: Vec<_> = function
        .expressions
        .iter()
        .filter_map(|(h, e)| matches!(e, Expression::Binary { .. }).then_some(h))
        .collect();
    assert_eq!(producers.len(), 2);
    for h in producers {
        function
            .expression_controls
            .insert(h, ArithmeticControl::NoContraction);
    }
    function.named_expressions.clear();
    validate(&module).unwrap();
    naga::compact::compact(&mut module, naga::compact::KeepUnused::No);
    let info = validate(&module).unwrap();
    let function = &module.entry_points[0].function;
    assert_eq!(function.expression_controls.len(), 1);
    let (&new_live, _) = function.expression_controls.first().unwrap();
    assert_ne!(old_live, new_live);
    assert!(matches!(
        function.expressions[new_live],
        Expression::Binary {
            op: BinaryOperator::Multiply,
            ..
        }
    ));
    let words = back::spv::write_vec(&module, &info, &Default::default(), None).unwrap();
    let mut cursor = 5;
    let mut decorations = 0;
    while cursor < words.len() {
        let n = (words[cursor] >> 16) as usize;
        assert!(n > 0);
        if words[cursor] & 65535 == 71 && words[cursor + 2] == 42 {
            decorations += 1;
        }
        cursor += n;
    }
    assert_eq!(decorations, 1);
    let decoded = naga::front::spv::Frontend::new(words.into_iter(), &Default::default())
        .parse()
        .unwrap();
    validate(&decoded).unwrap();
    assert_eq!(
        decoded
            .functions
            .iter()
            .map(|(_, f)| f.expression_controls.len())
            .sum::<usize>()
            + decoded
                .entry_points
                .iter()
                .map(|e| e.function.expression_controls.len())
                .sum::<usize>(),
        1
    );
}

#[test]
fn out_of_arena_control_is_rejected() {
    let mut module =
        naga::front::wgsl::parse_str("@compute @workgroup_size(1) fn main(){}").unwrap();
    let mut arena = naga::Arena::new();
    for _ in 0..100 {
        arena.append(
            Expression::Literal(naga::Literal::F32(1.0)),
            naga::Span::UNDEFINED,
        );
    }
    let handle = arena.append(
        Expression::Literal(naga::Literal::F32(1.0)),
        naga::Span::UNDEFINED,
    );
    module.entry_points[0]
        .function
        .expression_controls
        .insert(handle, ArithmeticControl::NoContraction);
    assert!(validate(&module).is_err());
}
#[test]
fn selected_entry_reachability() {
    let mut m=naga::front::wgsl::parse_str("@group(0) @binding(0) var<storage,read_write> out:f32; fn restricted(x:f32)->f32{return x*x;} fn caller(x:f32)->f32{if x>0.0{return restricted(x);}return x;} @compute @workgroup_size(1) fn dirty(){out=caller(out);} @compute @workgroup_size(1) fn clean(){out=out+1.0;}").unwrap();
    let (_, f) = m
        .functions
        .iter_mut()
        .find(|(_, f)| f.name.as_deref() == Some("restricted"))
        .unwrap();
    let h = f
        .expressions
        .iter()
        .find(|(_, e)| {
            matches!(
                e,
                Expression::Binary {
                    op: naga::BinaryOperator::Multiply,
                    ..
                }
            )
        })
        .unwrap()
        .0;
    f.expression_controls
        .insert(h, ArithmeticControl::NoContraction);
    let info = validate(&m).unwrap();
    for name in ["clean", "dirty"] {
        let pipe = back::hlsl::PipelineOptions {
            entry_point: Some((naga::ShaderStage::Compute, name.into())),
            ..Default::default()
        };
        let opts = back::hlsl::Options {
            fake_missing_bindings: true,
            ..Default::default()
        };
        let mut text = String::new();
        let result = {
            let mut w = back::hlsl::Writer::new(&mut text, &opts, &pipe);
            w.write(&m, &info, None)
        };
        assert_eq!(result.is_ok(), name == "clean");
        if name == "clean" {
            assert!(!text.contains("restricted("));
            assert!(!text.contains("caller("));
        }
        let pipe = back::msl::PipelineOptions {
            entry_point: Some((naga::ShaderStage::Compute, name.into())),
            ..Default::default()
        };
        let result = back::msl::write_string(&m, &info, &Default::default(), &pipe);
        assert_eq!(result.is_ok(), name == "clean");
        let opts = back::glsl::Options {
            version: back::glsl::Version::new_gles(310),
            ..Default::default()
        };
        let pipe = back::glsl::PipelineOptions {
            entry_point: name.into(),
            shader_stage: naga::ShaderStage::Compute,
            multiview: None,
        };
        let mut text = String::new();
        let result = {
            let mut w =
                back::glsl::Writer::new(&mut text, &m, &info, &opts, &pipe, Default::default())
                    .unwrap();
            w.write()
        };
        assert_eq!(result.is_ok(), name == "clean");
    }
    println!("PASS selected entry point: unreachable controlled callee and uncontrolled caller both omitted; reachable controls still reject unsupported target");
}
