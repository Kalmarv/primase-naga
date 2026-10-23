enable wgpu_ray_query;

@group(0) @binding(0) 
var acc_struct: acceleration_structure;

@compute @workgroup_size(1, 1, 1) 
fn main_candidate() {
    var rq_1: ray_query;

    let pos = vec3(0f);
    let dir = vec3<f32>(0f, 1f, 0f);
    rayQueryInitialize((&rq_1), acc_struct, RayDesc(4u, 255u, 0.1f, 100f, pos, dir));
    let intersection = rayQueryGetCandidateIntersection((&rq_1));
    if (intersection.kind == 3u) {
        rayQueryGenerateIntersection((&rq_1), 10f);
        return;
    } else {
        if (intersection.kind == 1u) {
            rayQueryConfirmIntersection((&rq_1));
            return;
        } else {
            rayQueryTerminate((&rq_1));
            return;
        }
    }
}
