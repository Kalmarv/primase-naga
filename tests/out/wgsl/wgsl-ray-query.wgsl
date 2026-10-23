enable wgpu_ray_query;

struct Output {
    visible: u32,
    normal: vec3<f32>,
}

@group(0) @binding(0) 
var acc_struct: acceleration_structure;
@group(0) @binding(1) 
var<storage, read_write> output: Output;

fn query_loop(pos: vec3<f32>, dir: vec3<f32>, acs: acceleration_structure) -> RayIntersection {
    var rq_1: ray_query;

    rayQueryInitialize((&rq_1), acs, RayDesc(4u, 255u, 0.1f, 100f, pos, dir));
    loop {
        let _e9 = rayQueryProceed((&rq_1));
        if _e9 {
        } else {
            break;
        }
        {
        }
    }
    let _e10 = rayQueryGetCommittedIntersection((&rq_1));
    return _e10;
}

fn get_torus_normal(world_point: vec3<f32>, intersection: RayIntersection) -> vec3<f32> {
    let local_point = (intersection.world_to_object * vec4<f32>(world_point, 1f));
    let point_on_guiding_line = (normalize(local_point.xy) * 2.4f);
    let world_point_on_guiding_line = (intersection.object_to_world * vec4<f32>(point_on_guiding_line, 0f, 1f));
    return normalize((world_point - world_point_on_guiding_line));
}

@compute @workgroup_size(1, 1, 1) 
fn main() {
    let pos_1 = vec3(0f);
    let dir_1 = vec3<f32>(0f, 1f, 0f);
    let _e7 = query_loop(pos_1, dir_1, acc_struct);
    output.visible = u32((_e7.kind == 0u));
    let _e18 = get_torus_normal((dir_1 * _e7.t), _e7);
    output.normal = _e18;
    return;
}

@compute @workgroup_size(1, 1, 1) 
fn main_candidate() {
    var rq: ray_query;

    let pos_2 = vec3(0f);
    let dir_2 = vec3<f32>(0f, 1f, 0f);
    rayQueryInitialize((&rq), acc_struct, RayDesc(4u, 255u, 0.1f, 100f, pos_2, dir_2));
    let intersection_1 = rayQueryGetCandidateIntersection((&rq));
    if (intersection_1.kind == 3u) {
        rayQueryGenerateIntersection((&rq), 10f);
        return;
    } else {
        if (intersection_1.kind == 1u) {
            rayQueryConfirmIntersection((&rq));
            return;
        } else {
            rayQueryTerminate((&rq));
            return;
        }
    }
}
