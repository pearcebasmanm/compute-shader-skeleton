/*
    1. Get GPUDevice (WHAT the code runs on)
       and
       GPUQueue (WHAT keeps track of WHAT to run **and** in WHAT order [e.g: please run the compute shader stage])
    2. Create GPUShaderModule (WHAT is ran on the GPU)
    3. We specify the INPUTs and the OUTPUTs of the shader code
    4. We specify WHAT binding# each INPUT/OUTPUT corresponds to in shader code (bind group layout)
    5. We specify WHAT group# each binding layout corresponds to (bind group)
    6. We specify EVERYTHING that will be part of the compute shader code, as part of the compute pipeline (the bind group layout + any constants you may want to add)
    7. We initialize the compute pipeline based on the sahder code + pipeline layout
    8. We create a compute pass,
       specify it the pipeline (inputs/output layouts+constants),
       the bind group (the buffer contents of inputs/outputs),
       the # of dispatches,
       what to do with the results (e.g: copy input buffer modified by the shader into the output buffer for reading),
       and finally we submit it to the Queue
    9. We run the command on the GPU
    10. We wait for it to finish and read back the results iwth mapAsync and getMappedRange
*/

use std::{env, fmt, fs};

use pollster::FutureExt;
use wgpu::util::DeviceExt;

#[derive(Debug)]
pub enum RequestDeviceError {
    AdapterCreationFailure,
    DeviceCreationFailure, // Add more error variants as needed
}

impl fmt::Display for RequestDeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestDeviceError::AdapterCreationFailure => write!(f, "Adapter creation failure"),
            RequestDeviceError::DeviceCreationFailure => {
                write!(f, "Device or queue creation failure")
            }
        }
    }
}

fn get_shader_code() -> String {
    let shader_code = match env::args().nth(1) {
        Some(shader_file_path) => {
            // Read the contents of the shader file
            match fs::read_to_string(&shader_file_path) {
                Ok(shader_file_content) => shader_file_content,
                Err(err) => {
                    eprintln!("Error reading shader file: {}", err);
                    std::process::exit(1);
                }
            }
        }
        None => {
            eprintln!("Usage: cargo run -- <shader_file_path>");
            std::process::exit(1);
        }
    };
    shader_code
}

async fn get_webgpu_device_and_queue() -> Result<(wgpu::Device, wgpu::Queue), RequestDeviceError> {
    // Instantiates instance of WebGPU
    let instance = wgpu::Instance::default();

    // `request_adapter` instantiates the general connection to the GPU
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .ok_or(RequestDeviceError::AdapterCreationFailure)?;

    // `request_device` instantiates the feature specific connection to the GPU, defining some parameters,
    //  `features` being the available features.
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("WebGPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
            },
            None,
        )
        .await
        .map_err(|_| RequestDeviceError::DeviceCreationFailure)?;

    Ok((device, queue))
}

fn create_shader_module(device: &wgpu::Device, shader_code: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shader Module"),
        source: wgpu::ShaderSource::Wgsl(shader_code.into()),
    })
}

fn create_shader_pipeline(
    device: &wgpu::Device,
    shader_module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Compute Pipeline"),
        layout: Some(layout),
        module: shader_module,
        entry_point: "main",
    })
}

fn main() {
    let shader_code = get_shader_code();
    let (device, queue) = get_webgpu_device_and_queue().block_on().unwrap();
    let shader_module = create_shader_module(&device, &shader_code);
    // TODO: Should allow for more complicated input data (e.g: 2D array)
    let mut input_data: [i32; 16] = [0; 16];

    // Iterate over the indices of the array and assign values
    for i in 0..16 {
        input_data[i] = i as i32;
    }
    let input_bytes = bytemuck::bytes_of(&input_data);
    // Create "input" buffer (what is to be processed by the shader)
    let input_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Input Buffer"),
        contents: input_bytes,
        // NOTE: Couldn't get UNIFORM to work...
        // As it's the SRC of a copy_buffer_to_buffer, we need COPY_SRC
        // As it's a var<storage> in the shader code, it's STORAGE
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });

    // Create the "output" buffer (where to place the results of the shader computation)
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Output Buffer"),
        size: input_bytes.len() as u64,
        // As it's the DST of a copy_buffer_to_buffer, we need COPY_DST
        // As we want to read from it(map_async), we need MAP_READ
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Specify: shader stage + groups + binding #s + buffer types
    // to then be used by the shader
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Bind Group Layout #0"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::COMPUTE,
                // @binding(0)
                binding: 0,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // Other entries here (e.g: buffers) that we want accessible from the shader code
            // ... haven't been able to make a separate output buffer work :|
            // it'd allow us to use 0 copies :|
        ],
    });

    // Specifying the buffers associated with the individual bindings in the binding group
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Bind Group #0"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: input_buffer.as_entire_binding(),
        }],
    });

    // Create pipeline layout
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        // [@group(0), @group(1), ...]
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let shader_pipeline = create_shader_pipeline(&device, &shader_module, &pipeline_layout);

    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut compute_pass = encoder.begin_compute_pass(&Default::default());
        compute_pass.set_pipeline(&shader_pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        // // Passing the input length as a param to the shader code
        // // Offset 0 specifies it's the first param
        // // The second param would have an offset of input_data.len()
        // compute_pass.set_push_constants(0, &[input_bytes.len() as u8]);
        compute_pass.dispatch_workgroups(input_bytes.len() as u32, 1, 1);
    }
    encoder.copy_buffer_to_buffer(
        &input_buffer,
        0,
        &output_buffer,
        0,
        input_bytes.len() as u64,
    );
    queue.submit(Some(encoder.finish()));

    let buffer_slice = output_buffer.slice(..);
    buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::Maintain::Wait);
    let output = buffer_slice.get_mapped_range().to_vec();
    println!("Output: {:?}", output);
}
