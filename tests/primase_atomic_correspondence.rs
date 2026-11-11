#![cfg(all(feature = "spv-in", feature = "spv-out", feature = "wgsl-in"))]
use naga::correspondence::*;
use naga::*;
fn validate(m: &Module) -> valid::ModuleInfo {
    valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
        .validate(m)
        .unwrap()
}
#[test]
fn primase_atomic_result_remap_and_emission() {
    let mut m=front::wgsl::parse_str("@group(0) @binding(0) var tex:texture_storage_2d<r32uint,atomic>; @group(0) @binding(1) var<storage,read_write> out:u32; @compute @workgroup_size(1) fn main(){ let p=out+1u; out=p; textureAtomicAdd(tex,vec2<i32>(0),p); }").unwrap();
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
    let dead = f
        .expressions
        .append(Expression::Literal(Literal::U32(12345)), Span::UNDEFINED);
    let old_result = f.expressions.append(
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
        *r = Some(old_result);
        *fun = AtomicFunction::Exchange { compare: None };
    }
    let before_return = f.body.len() - 1;
    assert!(matches!(f.body[before_return], Statement::Return { .. }));
    f.body.splice(
        before_return..before_return,
        Block::from_vec(vec![Statement::Store {
            pointer: ptr,
            value: old_result,
        }]),
    );
    f.named_expressions.clear();
    validate(&m);
    let mut normal = m.clone();
    compact::compact(&mut normal, compact::KeepUnused::No);
    let c = compact::compact_with_report(&mut m, compact::KeepUnused::No);
    assert_eq!(format!("{normal:?}"), format!("{m:?}"));
    let mapped = c
        .owners
        .iter()
        .find(|o| o.old == Owner::EntryPoint(0))
        .unwrap()
        .expressions
        .iter()
        .find(|(e, _)| *e == old_result)
        .unwrap()
        .1;
    let result = match mapped {
        Disposition::Survives(e) => {
            assert_eq!(e.owner, Owner::EntryPoint(0));
            e.expression
        }
        _ => panic!(),
    };
    assert_ne!(result, old_result);
    assert!(matches!(
        c.owners[0]
            .expressions
            .iter()
            .find(|(e, _)| *e == dead)
            .unwrap()
            .1,
        Disposition::Removed
    ));
    let info = validate(&m);
    let f = &m.entry_points[0].function;
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
    assert!(f.body.iter().any(|s|matches!(s,Statement::ImageAtomic{result:Some(r),fun:AtomicFunction::Exchange{compare:None},..} if *r==result)));
    let (words, report) =
        back::spv::write_vec_with_report(&m, &info, &Default::default(), None).unwrap();
    assert_eq!(
        words,
        back::spv::write_vec(&m, &info, &Default::default(), None).unwrap()
    );
    let id = match &report
        .values
        .iter()
        .find(|(s, _)| s.owner == Owner::EntryPoint(0) && s.expression == result)
        .unwrap()
        .1
    {
        EmittedValue::Value(id) => *id,
        _ => panic!(),
    };
    let mut cursor = 5;
    let mut atomic = None;
    let mut stores = vec![];
    while cursor < words.len() {
        let n = (words[cursor] >> 16) as usize;
        let op = words[cursor] & 65535;
        if op == 229 {
            atomic = Some(words[cursor + 2]);
        }
        if op == 62 {
            stores.push(words[cursor + 2]);
        }
        cursor += n;
    }
    assert_eq!(atomic, Some(id));
    assert!(stores.contains(&id));
    let plain = front::spv::Frontend::new(words.iter().copied(), &Default::default()).parse();
    let reported =
        front::spv::Frontend::new(words.iter().copied(), &Default::default()).parse_with_report();
    match (plain, reported) {
        (Ok(a), Ok((b, r))) => {
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
            validate(&b);
            assert_eq!(r.arithmetic.len(), 1);
            println!("PASS atomic import parity and ordinary Add map");
        }
        (Err(a), Err(b)) => {
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
            assert!(matches!(a, front::spv::Error::UnsupportedStorageClass(11)));
            println!("UNCHANGED unsupported atomic import: {a:?}");
        }
        other => panic!("import parity {other:?}"),
    }
    println!("PASS Primase image atomic exchange result maps to actual atomic result ID and Store value; compaction/emission parity");
}
