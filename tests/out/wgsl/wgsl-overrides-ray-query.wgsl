enable wgpu_ray_query;

override o: f32;

@group(0) @binding(0) 
var acc_struct: acceleration_structure;

@compute @workgroup_size(1, 1, 1) 
fn main() {
    var rq: ray_query;

    let desc = RayDesc(4u, 255u, (o * 17f), (o * 19f), vec3((o * 23f)), vec3<f32>((o * 29f), (o * 31f), (o * 37f)));
    rayQueryInitialize((&rq), acc_struct, desc);
    loop {
        let _e25 = rayQueryProceed((&rq));
        if _e25 {
        } else {
            break;
        }
        {
        }
    }
    return;
}
