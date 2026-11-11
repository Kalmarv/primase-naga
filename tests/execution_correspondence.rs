#![cfg(all(feature = "spv-in", feature = "spv-out", feature = "wgsl-in"))]

use naga::correspondence::*;
use naga::*;
fn validate(m: &Module) -> valid::ModuleInfo {
    valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
        .validate(m)
        .unwrap()
}
fn expr(f: &mut Function, e: Expression) -> Handle<Expression> {
    f.expressions.append(e, Span::UNDEFINED)
}
fn fun(ty: Handle<Type>) -> Function {
    let mut f = Function {
        arguments: vec![
            FunctionArgument {
                name: None,
                ty,
                binding: None
            };
            2
        ],
        result: Some(FunctionResult { ty, binding: None }),
        ..Default::default()
    };
    expr(&mut f, Expression::Literal(Literal::F32(99.)));
    let a = expr(&mut f, Expression::FunctionArgument(0));
    let b = expr(&mut f, Expression::FunctionArgument(1));
    let start = f.expressions.len();
    let alias = expr(
        &mut f,
        Expression::As {
            expr: a,
            kind: ScalarKind::Float,
            convert: Some(4),
        },
    );
    let _dead = expr(
        &mut f,
        Expression::Binary {
            op: BinaryOperator::Subtract,
            left: a,
            right: b,
        },
    );
    let producer = expr(
        &mut f,
        Expression::Binary {
            op: BinaryOperator::Multiply,
            left: alias,
            right: b,
        },
    );
    let value = expr(
        &mut f,
        Expression::Binary {
            op: BinaryOperator::Add,
            left: producer,
            right: a,
        },
    );
    f.body.push(
        Statement::Emit(f.expressions.range_from(start)),
        Span::UNDEFINED,
    );
    f.body
        .push(Statement::Return { value: Some(value) }, Span::UNDEFINED);
    f
}
fn fixture() -> Module {
    let mut m = front::wgsl::parse_str(
        "@compute @workgroup_size(1) fn first() {} @compute @workgroup_size(1) fn second() {}",
    )
    .unwrap();
    m.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::F64),
        },
        Span::UNDEFINED,
    );
    let ty = m.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::F32),
        },
        Span::UNDEFINED,
    );
    let _dead = m.functions.append(fun(ty), Span::UNDEFINED);
    let first = m.functions.append(fun(ty), Span::UNDEFINED);
    let second = m.functions.append(fun(ty), Span::UNDEFINED);
    for (ep, callee) in m.entry_points.iter_mut().zip([first, second]) {
        let f = &mut ep.function;
        expr(f, Expression::Literal(Literal::F32(999.)));
        let a = expr(f, Expression::Literal(Literal::F32(2.)));
        let b = expr(f, Expression::Literal(Literal::F32(3.)));
        let r = expr(f, Expression::CallResult(callee));
        f.body = Block::new();
        f.body.push(
            Statement::Call {
                function: callee,
                arguments: vec![a, b],
                result: Some(r),
            },
            Span::UNDEFINED,
        );
    }
    m
}
fn function(m: &Module, o: Owner) -> &Function {
    match o {
        Owner::Function(h) => &m.functions[h],
        Owner::EntryPoint(i) => &m.entry_points[i].function,
    }
}
fn args(e: &Expression) -> [Handle<Expression>; 2] {
    match *e {
        Expression::Binary { left, right, .. } => [left, right],
        Expression::Math {
            fun: MathFunction::Dot,
            arg,
            arg1: Some(b),
            ..
        } => [arg, b],
        _ => panic!("not arithmetic"),
    }
}
fn opcode(e: &Expression, scalar: ScalarKind) -> u32 {
    match *e {
        Expression::Binary { op, .. } => match (op, scalar) {
            (BinaryOperator::Add, ScalarKind::Float) => 129,
            (BinaryOperator::Subtract, ScalarKind::Float) => 131,
            (BinaryOperator::Multiply, ScalarKind::Float) => 133,
            (BinaryOperator::Add, _) => 128,
            (BinaryOperator::Subtract, _) => 130,
            (BinaryOperator::Multiply, _) => 132,
            _ => panic!(),
        },
        Expression::Math {
            fun: MathFunction::Dot,
            ..
        } => 148,
        _ => panic!(),
    }
}
fn instructions(words: &[u32]) -> Vec<&[u32]> {
    let mut result = Vec::new();
    let mut p = 5;
    while p < words.len() {
        let n = (words[p] >> 16) as usize;
        assert!(n > 0 && p + n <= words.len());
        result.push(&words[p..p + n]);
        p += n;
    }
    result
}
fn check_emit(m: &Module, words: &[u32], r: &EmissionReport) {
    assert_eq!(r.revision, REVISION);
    let inst = instructions(words);
    let info = validate(m);
    for a in &r.arithmetic {
        assert!(
            matches!(r.producer(a.producer),ArithmeticEmission::Direct(x) if x.result_id==a.result_id)
        );
        let f = function(m, a.producer.owner);
        let e = &f.expressions[a.producer.expression];
        let aa = args(e);
        assert_eq!(
            aa,
            [a.arguments[0].0.expression, a.arguments[1].0.expression]
        );
        let fi = match a.producer.owner {
            Owner::Function(h) => &info[h],
            Owner::EntryPoint(i) => info.get_entry_point(i),
        };
        let kind = fi[a.producer.expression]
            .ty
            .inner_with(&m.types)
            .scalar_kind()
            .unwrap();
        assert_eq!(a.opcode, opcode(e, kind));
        let x = inst
            .iter()
            .find(|i| i.len() == 5 && i[0] & 65535 == a.opcode && i[2] == a.result_id)
            .unwrap();
        assert_eq!(
            &x[1..],
            &[a.type_id, a.result_id, a.arguments[0].1, a.arguments[1].1]
        );
        let ty = inst
            .iter()
            .find(|i| matches!(i[0] & 65535, 21 | 22 | 23) && i[1] == a.type_id)
            .unwrap();
        if kind == ScalarKind::Float && ty[0] & 65535 == 22 {
            assert_eq!(
                ty[2],
                8 * fi[a.producer.expression]
                    .ty
                    .inner_with(&m.types)
                    .scalar_width()
                    .unwrap() as u32
            );
        }
        for (source, id) in a.arguments {
            assert_eq!(source.owner, a.producer.owner);
            assert!(r
                .values
                .iter()
                .any(|(s, d)| *s == source && matches!(d,EmittedValue::Value(i) if *i==id)));
        }
    }
}
fn check_import(words: &[u32]) -> (Module, ImportReport) {
    let opt = front::spv::Options::default();
    let ordinary = front::spv::Frontend::new(words.iter().copied(), &opt)
        .parse()
        .unwrap();
    let (m, r) = front::spv::Frontend::new(words.iter().copied(), &opt)
        .parse_with_report()
        .unwrap();
    assert_eq!(format!("{ordinary:?}"), format!("{m:?}"));
    validate(&m);
    let inst = instructions(words);
    let info = validate(&m);
    for a in &r.arithmetic {
        match a {
            ImportedArithmetic::Direct {
                function_id,
                result_id,
                type_id,
                producer,
                arguments,
            } => {
                assert!(inst
                    .iter()
                    .any(|i| i[0] & 65535 == 54 && i[2] == *function_id));
                let f = function(&m, producer.owner);
                let aa = args(&f.expressions[producer.expression]);
                assert_eq!(aa, [arguments[0].1.expression, arguments[1].1.expression]);
                assert_eq!(arguments[0].1.owner, producer.owner);
                assert_eq!(arguments[1].1.owner, producer.owner);
                let x = inst
                    .iter()
                    .find(|i| {
                        i.len() == 5
                            && i[2] == *result_id
                            && matches!(i[0] & 65535, 128..=133 | 148)
                    })
                    .unwrap();
                assert_eq!(
                    &x[1..],
                    &[*type_id, *result_id, arguments[0].0, arguments[1].0]
                );
                let fi = match producer.owner {
                    Owner::Function(h) => &info[h],
                    Owner::EntryPoint(i) => info.get_entry_point(i),
                };
                let ty = fi[producer.expression].ty.inner_with(&m.types);
                let scalar = ty.scalar_kind().unwrap();
                assert_eq!(
                    x[0] & 65535,
                    opcode(&f.expressions[producer.expression], scalar)
                );
                let st = inst
                    .iter()
                    .find(|i| matches!(i[0] & 65535, 21 | 22 | 23) && i[1] == *type_id)
                    .unwrap();
                let component = if st[0] & 65535 == 23 {
                    assert!(matches!(ty,TypeInner::Vector{size,..} if *size as u32==st[3]));
                    inst.iter()
                        .find(|i| matches!(i[0] & 65535, 21 | 22) && i[1] == st[2])
                        .unwrap()
                } else {
                    st
                };
                assert_eq!(component[2], 8 * ty.scalar_width().unwrap() as u32);
                assert_eq!(
                    component[0] & 65535,
                    if scalar == ScalarKind::Float { 22 } else { 21 }
                );
                if scalar != ScalarKind::Float {
                    assert_eq!(component[3], u32::from(scalar == ScalarKind::Sint));
                }
            }
            ImportedArithmetic::Unsupported { .. } => panic!("unexpected unsupported {a:?}"),
        }
    }
    (m, r)
}
#[test]
fn compaction_owners_and_operands() {
    let original = fixture();
    validate(&original);
    let mut normal = original.clone();
    compact::compact(&mut normal, compact::KeepUnused::No);
    let mut m = original.clone();
    let c = compact::compact_with_report(&mut m, compact::KeepUnused::No);
    assert_eq!(format!("{m:?}"), format!("{normal:?}"));
    validate(&m);
    assert_eq!(m.functions.len(), 2);
    assert!(matches!(c.owners[0].new, Disposition::Removed));
    assert!(c.owners[0]
        .expressions
        .iter()
        .all(|(_, d)| matches!(d, Disposition::Removed)));
    for row in &c.owners[1..3] {
        let owner = match row.new {
            Disposition::Survives(o) => o,
            _ => panic!(),
        };
        let old = match row.old {
            Owner::Function(h) => h,
            _ => panic!(),
        };
        let new = match owner {
            Owner::Function(h) => h,
            _ => panic!(),
        };
        assert_eq!(old.index(), new.index() + 1);
        let f = function(&m, owner);
        assert!(matches!(row.expressions[0].1, Disposition::Removed));
        assert!(matches!(row.expressions[4].1, Disposition::Removed));
        for (old_e, d) in &row.expressions {
            if let Disposition::Survives(e) = d {
                assert_eq!(e.owner, owner);
                assert!(e.expression.index() < old_e.index());
                if let Expression::Binary { .. } = original.functions[old].expressions[*old_e] {
                    let old_args = args(&original.functions[old].expressions[*old_e]);
                    let new_args = args(&f.expressions[e.expression]);
                    for i in 0..2 {
                        assert!(
                            matches!(row.expressions.iter().find(|(h,_)|*h==old_args[i]).unwrap().1,Disposition::Survives(r) if r.owner==owner&&r.expression==new_args[i])
                        );
                    }
                }
            }
        }
    }
    let mut keep = original.clone();
    let keep_r = compact::compact_with_report(&mut keep, compact::KeepUnused::Yes);
    let mut keep_plain = original.clone();
    compact::compact(&mut keep_plain, compact::KeepUnused::Yes);
    assert_eq!(format!("{keep:?}"), format!("{keep_plain:?}"));
    assert_eq!(keep.functions.len(), 3);
    assert!(keep_r
        .owners
        .iter()
        .all(|o| matches!(o.new, Disposition::Survives(_))));
}
#[test]
fn emission_values_selection_and_import() {
    let mut m = fixture();
    compact::compact(&mut m, compact::KeepUnused::No);
    let info = validate(&m);
    let options = back::spv::Options::default();
    let (words, r) = back::spv::write_vec_with_report(&m, &info, &options, None).unwrap();
    assert_eq!(
        words,
        back::spv::write_vec(&m, &info, &options, None).unwrap()
    );
    check_emit(&m, &words, &r);
    // Identity As aliases the argument ID; this is an explicit many-to-one value relation.
    for (h, f) in m.functions.iter() {
        let (alias, arg) = f
            .expressions
            .iter()
            .find_map(|(h, e)| {
                if let Expression::As { expr, .. } = e {
                    Some((h, *expr))
                } else {
                    None
                }
            })
            .unwrap();
        let get = |e| {
            r.values
                .iter()
                .find(|(s, _)| s.owner == Owner::Function(h) && s.expression == e)
                .unwrap()
        };
        assert!(matches!(
            r.producer(ExpressionRef {
                owner: Owner::Function(h),
                expression: alias
            }),
            ArithmeticEmission::ValueOnly(_)
        ));
        assert!(
            matches!((&get(alias).1,&get(arg).1),(EmittedValue::Value(a),EmittedValue::Value(b)) if a==b)
        );
    }
    let (im, ir) = check_import(&words);
    assert_eq!(ir.arithmetic.len(), r.arithmetic.len());
    // Entry SPIR-V bodies become module functions plus wrappers on import.
    assert!(ir.arithmetic.iter().all(|a| matches!(
        a,
        ImportedArithmetic::Direct {
            producer: ExpressionRef {
                owner: Owner::Function(_),
                ..
            },
            ..
        }
    )));
    validate(&im);
    let selected = back::spv::PipelineOptions {
        shader_stage: ShaderStage::Compute,
        entry_point: "first".into(),
    };
    let (sw, sr) = back::spv::write_vec_with_report(&m, &info, &options, Some(&selected)).unwrap();
    assert_eq!(
        sw,
        back::spv::write_vec(&m, &info, &options, Some(&selected)).unwrap()
    );
    assert!(sr
        .values
        .iter()
        .filter(|(s, _)| s.owner == Owner::EntryPoint(1))
        .all(|(_, v)| matches!(v, EmittedValue::OwnerNotEmitted)));
    check_emit(&m, &sw, &sr);
    let stage=front::wgsl::parse_str("fn only_fragment(x:f32)->f32{return dpdx(x)+x;} @fragment fn frag()->@location(0) f32{return only_fragment(1.);} @compute @workgroup_size(1) fn compute(){}").unwrap();
    let si = validate(&stage);
    let select = back::spv::PipelineOptions {
        shader_stage: ShaderStage::Compute,
        entry_point: "compute".into(),
    };
    let (ss, rs) = back::spv::write_vec_with_report(&stage, &si, &options, Some(&select)).unwrap();
    assert_eq!(
        ss,
        back::spv::write_vec(&stage, &si, &options, Some(&select)).unwrap()
    );
    let sh = stage.functions.iter().next().unwrap().0;
    assert!(rs
        .values
        .iter()
        .filter(|(s, _)| s.owner == Owner::Function(sh))
        .all(|(_, v)| matches!(v, EmittedValue::OwnerNotEmitted)));
}
#[test]
fn direct_entry_arithmetic() {
    let options = back::spv::Options::default();
    // Exercise direct producers in an entry body, vector arithmetic, integer signs, and Dot.
    let variants=front::wgsl::parse_str("var<private> x:vec2<f32>; var<private> y:f32; var<private> z:u32; @compute @workgroup_size(1) fn main(){ x=x+vec2<f32>(1.); x=x-vec2<f32>(2.); x=x*vec2<f32>(3.); y=dot(x,x); z=z+1u; z=z-2u; z=z*3u; }").unwrap();
    let vi = validate(&variants);
    let (vw, vr) = back::spv::write_vec_with_report(&variants, &vi, &options, None).unwrap();
    assert_eq!(vr.arithmetic.len(), 7);
    check_emit(&variants, &vw, &vr);
    assert_eq!(
        vw,
        back::spv::write_vec(&variants, &vi, &options, None).unwrap()
    );
    let (imported, vir) = check_import(&vw);
    assert_eq!(vir.arithmetic.len(), 7);
    assert!(vir.arithmetic.iter().all(|a| matches!(a, ImportedArithmetic::Direct {producer: ExpressionRef {owner: Owner::Function(h),..},..} if function_called_by_entry(&imported, *h))));
}
fn function_called_by_entry(m: &Module, target: Handle<Function>) -> bool {
    m.entry_points.iter().any(|ep| {
        ep.function
            .body
            .iter()
            .any(|s| matches!(s, Statement::Call {function,..} if *function == target))
    })
}
#[test]
fn adapted_operands_and_errors() {
    // Authored signedness-adapting input: explicitly unsupported correspondence.
    let mut u = vec![0x07230203, 0x10000, 0, 20, 0];
    let mut push = |op: u32, values: &[u32]| {
        u.push(((values.len() as u32 + 1) << 16) | op);
        u.extend(values);
    };
    push(17, &[1]);
    push(14, &[0, 1]);
    push(15, &[5, 10, 0x6e69616d, 0]);
    push(16, &[10, 17, 1, 1, 1]);
    push(19, &[1]);
    push(33, &[2, 1]);
    push(21, &[3, 32, 1]);
    push(21, &[4, 32, 0]);
    push(43, &[3, 5, 1]);
    push(43, &[3, 6, 2]);
    push(54, &[1, 10, 0, 2]);
    push(248, &[11]);
    push(128, &[4, 12, 5, 6]);
    push(253, &[]);
    push(56, &[]);
    let (um, ur) = front::spv::Frontend::new(u.iter().copied(), &Default::default())
        .parse_with_report()
        .unwrap();
    validate(&um);
    assert!(matches!(
        ur.arithmetic.as_slice(),
        [ImportedArithmetic::Unsupported { result_id: 12, .. }]
    ));
    let plain = front::spv::Frontend::new(u.iter().copied(), &Default::default())
        .parse()
        .unwrap();
    assert_eq!(format!("{plain:?}"), format!("{um:?}"));
    let mut bad = u.clone();
    bad[0] = 0;
    check_parse_error(&bad);
    let mut bad = u.clone();
    bad.truncate(9);
    check_parse_error(&bad);
    let mut bad = u;
    bad.splice(5..5, [2 << 16 | 17, 4]);
    check_parse_error(&bad); // Addresses capability
    let m = front::wgsl::parse_str(
        "enable f16; var<private> x:f16; @compute @workgroup_size(1) fn main(){x=x+1h;}",
    )
    .unwrap();
    let info = validate(&m);
    let restricted = back::spv::Options {
        capabilities: Some(Default::default()),
        ..Default::default()
    };
    assert_eq!(
        format!(
            "{:?}",
            back::spv::write_vec(&m, &info, &restricted, None).unwrap_err()
        ),
        format!(
            "{:?}",
            back::spv::write_vec_with_report(&m, &info, &restricted, None).unwrap_err()
        )
    );
    let missing = back::spv::PipelineOptions {
        shader_stage: ShaderStage::Compute,
        entry_point: "absent".into(),
    };
    let options = back::spv::Options::default();
    assert_eq!(
        format!(
            "{:?}",
            back::spv::write_vec(&m, &info, &options, Some(&missing)).unwrap_err()
        ),
        format!(
            "{:?}",
            back::spv::write_vec_with_report(&m, &info, &options, Some(&missing)).unwrap_err()
        )
    );
}

fn check_parse_error(words: &[u32]) {
    let options = front::spv::Options {
        strict_capabilities: true,
        ..Default::default()
    };
    let a = front::spv::Frontend::new(words.iter().copied(), &options)
        .parse()
        .unwrap_err();
    let b = front::spv::Frontend::new(words.iter().copied(), &options)
        .parse_with_report()
        .unwrap_err();
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}
#[test]
fn final_function_reordering() {
    let mut words = vec![0x07230203, 0x10000, 0, 30, 0];
    let mut push = |op: u32, a: &[u32]| {
        words.push(((a.len() as u32 + 1) << 16) | op);
        words.extend(a);
    };
    push(17, &[1]);
    push(14, &[0, 1]);
    push(15, &[5, 10, 0x6e69616d, 0]);
    push(16, &[10, 17, 1, 1, 1]);
    push(19, &[1]);
    push(33, &[2, 1]);
    push(21, &[3, 32, 0]);
    push(43, &[3, 5, 1]);
    push(43, &[3, 6, 2]);
    push(54, &[1, 10, 0, 2]);
    push(248, &[11]);
    push(128, &[3, 12, 5, 6]);
    push(57, &[1, 13, 20]);
    push(253, &[]);
    push(56, &[]);
    push(54, &[1, 20, 0, 2]);
    push(248, &[21]);
    push(130, &[3, 22, 6, 5]);
    push(253, &[]);
    push(56, &[]);
    let (m, r) = check_import(&words);
    assert_eq!(r.arithmetic.len(), 2);
    for a in r.arithmetic {
        match a {
            ImportedArithmetic::Direct {
                function_id,
                result_id,
                producer:
                    ExpressionRef {
                        owner: Owner::Function(h),
                        ..
                    },
                ..
            } => {
                if result_id == 12 {
                    assert_eq!(function_id, 10);
                    assert_eq!(h.index(), 1);
                    assert!(function_called_by_entry(&m, h));
                } else {
                    assert_eq!(result_id, 22);
                    assert_eq!(function_id, 20);
                    assert_eq!(h.index(), 0);
                }
            }
            _ => panic!("expected reordered function owner"),
        }
    }
}
#[test]
fn module_arena_dispositions() {
    let mut m = front::wgsl::parse_str("@compute @workgroup_size(1) fn main(){}").unwrap();
    let dead_ty = m.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::F64),
        },
        Span::UNDEFINED,
    );
    let ty = m.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::U32),
        },
        Span::UNDEFINED,
    );
    let dead = m
        .global_expressions
        .append(Expression::Literal(Literal::U32(99)), Span::UNDEFINED);
    let live = m
        .global_expressions
        .append(Expression::Literal(Literal::U32(1)), Span::UNDEFINED);
    let dc = m.constants.append(
        Constant {
            name: None,
            ty,
            init: dead,
        },
        Span::UNDEFINED,
    );
    let lc = m.constants.append(
        Constant {
            name: Some("retained".into()),
            ty,
            init: live,
        },
        Span::UNDEFINED,
    );
    let od = m.overrides.append(
        Override {
            name: None,
            id: None,
            ty,
            init: Some(dead),
        },
        Span::UNDEFINED,
    );
    let ol = m.overrides.append(
        Override {
            name: Some("size".into()),
            id: None,
            ty,
            init: Some(live),
        },
        Span::UNDEFINED,
    );
    let size = m
        .global_expressions
        .append(Expression::Override(ol), Span::UNDEFINED);
    m.entry_points[0].workgroup_size_overrides = Some([Some(size), None, None]);
    let gd = m.global_variables.append(
        GlobalVariable {
            name: None,
            space: AddressSpace::Private,
            binding: None,
            ty,
            init: Some(dead),
            memory_decorations: MemoryDecorations::empty(),
        },
        Span::UNDEFINED,
    );
    let gl = m.global_variables.append(
        GlobalVariable {
            name: None,
            space: AddressSpace::Private,
            binding: None,
            ty,
            init: Some(live),
            memory_decorations: MemoryDecorations::empty(),
        },
        Span::UNDEFINED,
    );
    let f = &mut m.entry_points[0].function;
    let pointer = expr(f, Expression::GlobalVariable(gl));
    let value = expr(f, Expression::Constant(lc));
    f.body = Block::new();
    f.body
        .push(Statement::Store { pointer, value }, Span::UNDEFINED);
    validate(&m);
    let mut plain = m.clone();
    compact::compact(&mut plain, compact::KeepUnused::No);
    let r = compact::compact_with_report(&mut m, compact::KeepUnused::No);
    validate(&m);
    assert_eq!(format!("{m:?}"), format!("{plain:?}"));
    fn removed<T>(rows: &[HandleRemap<T>], old: Handle<T>) {
        assert!(matches!(
            rows.iter().find(|r| r.old == old).unwrap().new,
            Disposition::Removed
        ));
    }
    fn moved<T>(rows: &[HandleRemap<T>], old: Handle<T>) -> Handle<T> {
        match rows.iter().find(|r| r.old == old).unwrap().new {
            Disposition::Survives(h) => {
                assert!(h.index() < old.index());
                h
            }
            _ => panic!("removed"),
        }
    }
    removed(&r.types, dead_ty);
    let ty = moved(&r.types, ty);
    removed(&r.constants, dc);
    let constant = moved(&r.constants, lc);
    removed(&r.overrides, od);
    let ov = moved(&r.overrides, ol);
    removed(&r.globals, gd);
    let global = moved(&r.globals, gl);
    removed(&r.global_expressions, dead);
    let live = moved(&r.global_expressions, live);
    let size = moved(&r.global_expressions, size);
    assert_eq!(m.constants[constant].ty, ty);
    assert_eq!(m.constants[constant].init, live);
    assert_eq!(m.overrides[ov].init, Some(live));
    assert_eq!(m.global_variables[global].init, Some(live));
    assert!(matches!(m.global_expressions[size],Expression::Override(h) if h==ov));
    assert_eq!(
        m.entry_points[0].workgroup_size_overrides,
        Some([Some(size), None, None])
    );
}
#[test]
fn no_value_and_expanded_lowering() {
    let mut m = front::wgsl::parse_str(
        "var<private> v:vec2<f32>; @compute @workgroup_size(1) fn main(){v=v*2.;}",
    )
    .unwrap();
    let f = &mut m.entry_points[0].function;
    let a = expr(f, Expression::Literal(Literal::F32(1.)));
    let b = expr(f, Expression::Literal(Literal::F32(2.)));
    let dead = expr(
        f,
        Expression::Binary {
            op: BinaryOperator::Add,
            left: a,
            right: b,
        },
    );
    let info = validate(&m);
    let options = back::spv::Options::default();
    let (words, r) = back::spv::write_vec_with_report(&m, &info, &options, None).unwrap();
    assert_eq!(
        words,
        back::spv::write_vec(&m, &info, &options, None).unwrap()
    );
    assert!(matches!(
        r.producer(ExpressionRef {
            owner: Owner::EntryPoint(0),
            expression: dead
        }),
        ArithmeticEmission::NoValueId
    ));
    assert!(matches!(
        r.producer(ExpressionRef {
            owner: Owner::EntryPoint(999),
            expression: dead
        }),
        ArithmeticEmission::UnknownSource
    ));
    let (mul, _) = m.entry_points[0]
        .function
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
        .unwrap();
    assert!(matches!(
        r.producer(ExpressionRef {
            owner: Owner::EntryPoint(0),
            expression: mul
        }),
        ArithmeticEmission::ValueOnly(_)
    ));
    assert!(r.arithmetic.is_empty());
}
