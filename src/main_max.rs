// mod minimal;
mod repeatable;

use repeatable::Runner;
async fn run() {
    let runner = Runner::new(include_str!("shaders/inplace_add.wgsl"), "main").await;
    let inputs: [&[f32]; 3] = [&[1.0, 2.0], &[10.0, -12.0], &[1.0]];
    println!("f32 + 100.0");
    for input in inputs {
        let output = runner.run(input).await;
        println!("{input:?} -> {output:?}");
    }

    let runner = Runner::new(include_str!("shaders/inplace_mult.wgsl"), "main").await;
    let inputs: [&[i32]; 4] = [&[1, 2, 3, 4, 5], &[10, -12, 2], &[1], &[0]];
    println!("i32 * 3");
    for input in inputs {
        let output = runner.run(input).await;
        println!("{input:?} -> {output:?}");
    }
}

fn main() {
    pollster::block_on(run());
}
