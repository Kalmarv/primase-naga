enable wgpu_ray_query;
enable wgpu_ray_query_vertex_return;

struct Output {
    committed: array<vec3<f32>, 3>,
    candidate: array<vec3<f32>, 3>,
}

@group(0) @binding(0) 
var acc_struct: acceleration_structure<vertex_return>;
@group(0) @binding(1) 
var<storage, read_write> output: Output;

@compute @workgroup_size(1, 1, 1) 
fn main() {
    var rq: ray_query<vertex_return>;

    rayQueryInitialize((&rq), acc_struct, RayDesc(1u, 255u, 0.1f, 100f, vec3(0f), vec3<f32>(0f, 1f, 0f)));
    loop {
        let _e13 = rayQueryProceed((&rq));
        if _e13 {
        } else {
            break;
        }
        {
            let _e16 = getCandidateHitVertexPositions((&rq));
            output.candidate = _e16;
            rayQueryConfirmIntersection((&rq));
        }
    }
    let _e19 = getCommittedHitVertexPositions((&rq));
    output.committed = _e19;
    return;
}
