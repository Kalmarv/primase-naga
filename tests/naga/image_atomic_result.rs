/*!
Tests of the old value that an image atomic operation can give back
([`Statement::ImageAtomic::result`]).

No front end produces such a result (WGSL's texture atomics return nothing), so the
modules here are parsed from WGSL and then given results: each `ImageAtomic` statement
gets an [`Expression::AtomicResult`], which is stored to a local variable of its own.
*/

#![cfg(all(
    feature = "wgsl-in",
    any(spv_out, hlsl_out, msl_out, glsl_out, wgsl_out)
))]

use naga::{
    valid::{Capabilities, ModuleInfo, ValidationFlags, Validator},
    Block, Expression, LocalVariable, Module, Scalar, Span, Statement, Type, TypeInner,
};

const SOURCE: &str = "
@group(0) @binding(0)
var image_u: texture_storage_2d<r32uint, atomic>;
@group(0) @binding(1)
var image_s: texture_storage_2d<r32sint, atomic>;

@compute
@workgroup_size(2)
fn cs_main(@builtin(local_invocation_id) id: vec3<u32>) {
    textureAtomicMax(image_u, vec2<i32>(0, 0), 1u);
    textureAtomicAdd(image_u, vec2<i32>(0, 0), 2u);
    textureAtomicXor(image_u, vec2<i32>(0, 0), 3u);
    textureAtomicMin(image_s, vec2<i32>(0, 0), 1i);
    textureAtomicOr(image_s, vec2<i32>(0, 0), 2i);
}
";

fn parse(source: &str) -> Module {
    naga::front::wgsl::parse_str(source).unwrap_or_else(|e| {
        panic!(
            "expected WGSL to parse successfully:\n{}",
            e.emit_to_string(source)
        )
    })
}

/// Give every `ImageAtomic` of the module's first entry point a result, and store the
/// result to a local variable. `change` can alter the result expression of each.
fn with_results(module: &mut Module, mut change: impl FnMut(&mut Expression)) {
    let function = &mut module.entry_points[0].function;
    let old = core::mem::take(&mut function.body);
    let mut body = Block::new();
    for (statement, span) in old.span_into_iter() {
        let Statement::ImageAtomic {
            image,
            coordinate,
            array_index,
            fun,
            value,
            ..
        } = statement
        else {
            body.push(statement, span);
            continue;
        };
        let scalar = match function.expressions[value] {
            Expression::Literal(naga::Literal::U32(_)) => Scalar::U32,
            Expression::Literal(naga::Literal::I32(_)) => Scalar::I32,
            ref other => panic!("unexpected value {other:?}"),
        };
        let ty = module.types.insert(
            Type {
                name: None,
                inner: TypeInner::Scalar(scalar),
            },
            span,
        );
        let mut expression = Expression::AtomicResult {
            ty,
            comparison: false,
        };
        change(&mut expression);
        let result = function.expressions.append(expression, span);
        body.push(
            Statement::ImageAtomic {
                image,
                coordinate,
                array_index,
                fun,
                value,
                result: Some(result),
            },
            span,
        );
        let local = function.local_variables.append(
            LocalVariable {
                name: Some("old".into()),
                ty,
                init: None,
            },
            span,
        );
        let pointer = function
            .expressions
            .append(Expression::LocalVariable(local), span);
        body.push(
            Statement::Store {
                pointer,
                value: result,
            },
            span,
        );
    }
    function.body = body;
}

fn validate(module: &Module) -> Result<ModuleInfo, naga::WithSpan<naga::valid::ValidationError>> {
    Validator::new(ValidationFlags::all(), Capabilities::all()).validate(module)
}

fn valid_module() -> (Module, ModuleInfo) {
    let mut module = parse(SOURCE);
    with_results(&mut module, |_| {});
    let info = validate(&module).expect("a result of the right type validates");
    (module, info)
}

#[test]
fn a_result_of_the_texels_type_validates() {
    valid_module();
}

#[test]
fn a_result_of_another_type_is_invalid() {
    let mut module = parse(SOURCE);
    let i32_ty = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::I32),
        },
        Span::UNDEFINED,
    );
    let f32_ty = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::F32),
        },
        Span::UNDEFINED,
    );
    // the first operation is on a `u32` image: an `i32` or `f32` result does not match
    for ty in [i32_ty, f32_ty] {
        let mut module = module.clone();
        let mut first = true;
        // only the first ImageAtomic changes: the rest keep their own types
        with_results(&mut module, |expression| {
            if core::mem::take(&mut first) {
                *expression = Expression::AtomicResult {
                    ty,
                    comparison: false,
                };
            }
        });
        let err = validate(&module).expect_err("a result of another type is invalid");
        assert!(
            format!("{err:?}").contains("ResultTypeMismatch"),
            "unexpected error: {err:?}"
        );
    }
}

#[test]
fn a_comparison_result_is_invalid() {
    let mut module = parse(SOURCE);
    with_results(&mut module, |expression| {
        if let Expression::AtomicResult { comparison, .. } = expression {
            *comparison = true;
        }
    });
    assert!(validate(&module).is_err());
}

#[test]
fn a_result_is_given_by_one_statement_only() {
    let mut module = parse(SOURCE);
    with_results(&mut module, |_| {});
    // the second operation gives the result of the first
    let mut results = module.entry_points[0]
        .function
        .body
        .iter()
        .filter_map(|s| match s {
            Statement::ImageAtomic { result, .. } => *result,
            _ => None,
        });
    let first = results.next().unwrap();
    let mut seen = false;
    for statement in module.entry_points[0].function.body.iter_mut() {
        if let Statement::ImageAtomic { result, .. } = statement {
            if core::mem::replace(&mut seen, true) {
                *result = Some(first);
                break;
            }
        }
    }
    let err = validate(&module).expect_err("a result given twice is invalid");
    assert!(
        format!("{err:?}").contains("ResultAlreadyPopulated"),
        "unexpected error: {err:?}"
    );
}

#[test]
fn a_result_that_is_not_an_atomic_result_is_invalid() {
    let mut module = parse(SOURCE);
    with_results(&mut module, |expression| {
        *expression = Expression::Literal(naga::Literal::U32(0));
    });
    assert!(validate(&module).is_err());
}

#[test]
fn a_64_bit_image_has_no_result() {
    let mut module = parse(include_str!("../in/wgsl/atomicTexture-int64.wgsl"));
    let function = &mut module.entry_points[0].function;
    let u64_ty = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::U64),
        },
        Span::UNDEFINED,
    );
    let result = function.expressions.append(
        Expression::AtomicResult {
            ty: u64_ty,
            comparison: false,
        },
        Span::UNDEFINED,
    );
    for statement in function.body.iter_mut() {
        if let Statement::ImageAtomic { result: slot, .. } = statement {
            *slot = Some(result);
            break;
        }
    }
    let err = validate(&module).expect_err("a 64-bit operation has no result");
    assert!(
        format!("{err:?}").contains("ImageResultUnsupported"),
        "unexpected error: {err:?}"
    );
}

#[cfg(spv_out)]
#[test]
fn spirv_uses_the_instructions_result() {
    use rspirv::binary::Disassemble;

    let (module, info) = valid_module();
    let words =
        naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)
            .unwrap();
    let text = rspirv::dr::load_words(&words).unwrap().disassemble();
    // the stored value is the atomic instruction's own result
    for (op, id) in text
        .lines()
        .filter(|l| l.contains("= OpAtomic"))
        .map(|l| {
            let (id, rest) = l.trim().split_once(" = ").unwrap();
            (
                rest.split_whitespace().next().unwrap().to_owned(),
                id.to_owned(),
            )
        })
        .collect::<Vec<_>>()
    {
        assert!(
            text.lines()
                .any(|l| l.trim().starts_with("OpStore") && l.trim().ends_with(&id)),
            "{op}'s result {id} is not stored:\n{text}"
        );
    }
    assert_eq!(text.matches("= OpAtomic").count(), 5, "{text}");
}

#[cfg(hlsl_out)]
#[test]
fn hlsl_passes_the_old_value_as_the_last_argument() {
    let (module, info) = valid_module();
    let options = naga::back::hlsl::Options::default();
    let mut text = String::new();
    naga::back::hlsl::Writer::new(&mut text, &options, &Default::default())
        .write(&module, &info, None)
        .unwrap();
    assert!(
        text.contains("uint _e26; InterlockedMax(image_u[int2(int(0), int(0))], 1u, _e26);"),
        "{text}"
    );
    assert!(
        text.contains("int _e32; InterlockedMin(image_s[int2(int(0), int(0))], int(1), _e32);"),
        "{text}"
    );
}

#[cfg(msl_out)]
#[test]
fn msl_takes_the_first_component_of_the_old_value() {
    let (module, info) = valid_module();
    let options = naga::back::msl::Options {
        lang_version: (3, 1),
        fake_missing_bindings: true,
        ..Default::default()
    };
    let (text, _) = naga::back::msl::write_string(
        &module,
        &info,
        &options,
        &naga::back::msl::PipelineOptions::default(),
    )
    .unwrap();
    assert!(
        text.contains(
            "uint _e26 = image_u.atomic_fetch_max(metal::uint2(metal::int2(0, 0)), 1u).x;"
        ),
        "{text}"
    );
    assert!(
        text.contains("int _e32 = image_s.atomic_fetch_min(metal::uint2(metal::int2(0, 0)), 1).x;"),
        "{text}"
    );
}

#[cfg(glsl_out)]
#[test]
fn glsl_declares_the_old_value() {
    let (module, info) = valid_module();
    let options = naga::back::glsl::Options {
        version: naga::back::glsl::Version::Desktop(420),
        ..Default::default()
    };
    let pipeline = naga::back::glsl::PipelineOptions {
        shader_stage: naga::ShaderStage::Compute,
        entry_point: "cs_main".into(),
        multiview: None,
    };
    let mut text = String::new();
    naga::back::glsl::Writer::new(
        &mut text,
        &module,
        &info,
        &options,
        &pipeline,
        Default::default(),
    )
    .unwrap()
    .write()
    .unwrap();
    assert!(
        text.contains("uint _e26 = imageAtomicMax(_group_0_binding_0_cs, ivec2(0, 0), 1u);"),
        "{text}"
    );
    assert!(
        text.contains("int _e32 = imageAtomicMin(_group_0_binding_1_cs, ivec2(0, 0), 1);"),
        "{text}"
    );
}

#[cfg(wgsl_out)]
#[test]
fn wgsl_refuses_a_result() {
    let (module, info) = valid_module();
    let err =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .expect_err("WGSL's texture atomics return nothing");
    assert!(
        err.to_string().contains("texture atomic result"),
        "unexpected error: {err}"
    );
    // without results it still writes
    let module = parse(SOURCE);
    let info = validate(&module).unwrap();
    naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty()).unwrap();
}
