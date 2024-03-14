@group(0)
@binding(0)
var<storage, read_write> buffer: array<f32>;

@compute
@workgroup_size(1)
fn main(@builtin(global_invocation_id) index_3d: vec3<u32>) {
    buffer[index_3d.x] += 100.0;
}
