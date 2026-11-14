use naga::*;
fn validate(m: &Module) -> valid::ModuleInfo {
    valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
        .validate(m)
        .unwrap()
}
#[test]
fn image_atomic_result_and_ray_query_compatibility() {
    let mut m=front::wgsl::parse_str("override a:u32=1u; @group(0) @binding(0) var tex:texture_storage_2d<r32uint,atomic>; @group(0) @binding(1) var<storage,read_write> out:u32; @compute @workgroup_size(1) fn main(){ let p=out+a; out=p; textureAtomicAdd(tex,vec2<i32>(0),p); }").unwrap();
    let ty = m
        .global_variables
        .iter()
        .find(|(_, g)| g.name.as_deref() == Some("out"))
        .unwrap()
        .1
        .ty;
    let f = &mut m.entry_points[0].function;
    let ptr = f
        .expressions
        .iter()
        .find_map(|(h, e)| {
            if let Expression::GlobalVariable(g) = e {
                if m.global_variables[*g].name.as_deref() == Some("out") {
                    Some(h)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    let add = f
        .expressions
        .iter()
        .find(|(_, e)| {
            matches!(
                e,
                Expression::Binary {
                    op: BinaryOperator::Add,
                    ..
                }
            )
        })
        .unwrap()
        .0;
    f.expression_controls
        .insert(add, ArithmeticControl::NoContraction);
    let result = f.expressions.append(
        Expression::AtomicResult {
            ty,
            comparison: false,
        },
        Span::UNDEFINED,
    );
    let atomic = f
        .body
        .iter_mut()
        .find(|s| matches!(s, Statement::ImageAtomic { .. }))
        .unwrap();
    if let Statement::ImageAtomic { result: r, fun, .. } = atomic {
        *r = Some(result);
        *fun = AtomicFunction::Exchange { compare: None };
    }
    f.body.push(
        Statement::Store {
            pointer: ptr,
            value: result,
        },
        Span::UNDEFINED,
    );
    let info = validate(&m);
    let (specialized, _) =
        back::pipeline_constants::process_overrides(&m, &info, None, &Default::default()).unwrap();
    let mut m = specialized.into_owned();
    m.entry_points[0].function.named_expressions.clear();
    compact::compact(&mut m, compact::KeepUnused::No);
    let info = validate(&m);
    let f = &m.entry_points[0].function;
    assert_eq!(f.expression_controls.len(), 1);
    let result = f
        .body
        .iter()
        .find_map(|s| {
            if let Statement::ImageAtomic {
                result: Some(r),
                fun: AtomicFunction::Exchange { compare: None },
                ..
            } = s
            {
                Some(*r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(matches!(
        f.expressions[result],
        Expression::AtomicResult {
            comparison: false,
            ..
        }
    ));
    assert!(f
        .body
        .iter()
        .any(|s| matches!(s,Statement::Store{value,..} if *value==result)));
    let words = back::spv::write_vec(&m, &info, &Default::default(), None).unwrap();
    let mut cursor = 5;
    let mut atomic = 0;
    let mut controls = 0;
    while cursor < words.len() {
        let n = (words[cursor] >> 16) as usize;
        let op = words[cursor] & 65535;
        if op == 229 {
            atomic += 1;
        }
        if op == 71 && words[cursor + 2] == 42 {
            controls += 1;
        }
        cursor += n;
    }
    assert_eq!((atomic, controls), (1, 1));
    let mut text = String::new();
    let opts = back::hlsl::Options {
        shader_model: back::hlsl::ShaderModel::V6_0,
        fake_missing_bindings: true,
        ..Default::default()
    };
    back::hlsl::Writer::new(&mut text, &opts, &Default::default())
        .write(&m, &info, None)
        .unwrap();
    assert!(text.contains("InterlockedExchange"));
    println!("PASS Primase image atomic exchange result + controlled integer Add: validate, specialize, compact, SPIR-V and HLSL emit, result consumer retained");
    let ray="enable wgpu_ray_query; @group(0) @binding(0) var acc:acceleration_structure; @compute @workgroup_size(1) fn main(){var q:ray_query; rayQueryInitialize(&q,acc,RayDesc(0u,255u,0.1,100.,vec3<f32>(0.),vec3<f32>(1.))); if rayQueryProceed(&q){ rayQueryGenerateIntersection(&q,1.); rayQueryConfirmIntersection(&q); } rayQueryTerminate(&q);}";
    let m = front::wgsl::parse_str(ray).unwrap();
    let info = validate(&m);
    let text = back::wgsl::write_string(&m, &info, back::wgsl::WriterFlags::empty()).unwrap();
    for token in [
        "rayQueryInitialize",
        "rayQueryProceed",
        "rayQueryGenerateIntersection",
        "rayQueryConfirmIntersection",
        "rayQueryTerminate",
    ] {
        assert!(text.contains(token));
    }
    validate(&front::wgsl::parse_str(&text).unwrap());
    println!("PASS Primase ray-query custom WGSL writer/lowerer round trip");
}
