enable wgpu_ray_query;
enable wgpu_ray_query_vertex_return;

@group(0) @binding(0)
var acc_struct: acceleration_structure<vertex_return>;

struct Output {
    committed: array<vec3<f32>, 3>,
    candidate: array<vec3<f32>, 3>,
}

@group(0) @binding(1)
var<storage, read_write> output: Output;

@compute @workgroup_size(1)
fn main() {
    var rq: ray_query<vertex_return>;
    rayQueryInitialize(&rq, acc_struct, RayDesc(0x01u, 0xFFu, 0.1, 100.0, vec3<f32>(0.0), vec3<f32>(0.0, 1.0, 0.0)));
    while (rayQueryProceed(&rq)) {
        output.candidate = getCandidateHitVertexPositions(&rq);
        rayQueryConfirmIntersection(&rq);
    }
    output.committed = getCommittedHitVertexPositions(&rq);
}
