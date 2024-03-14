use wgpu::util::DeviceExt;

pub struct Runner {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

impl Runner {
    /// Defines the variables that don't depend on the specific slice being computed over
    pub async fn new(shader: &str, entry_point: &str) -> Self {
        let (device, queue) = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .unwrap()
            .request_device(&Default::default(), None)
            .await
            .unwrap();
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                }),
            ),
            module: &device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(shader.into()),
            }),
            entry_point,
        });
        Self {
            device,
            queue,
            bind_group_layout,
            pipeline,
        }
    }

    /// defines the input-dependant variables and runs the computation
    pub async fn run<T: bytemuck::Pod>(&self, input: &[T]) -> Vec<T> {
        assert!(!input.is_empty(), "Input cannot be empty");

        let input_bytes: &[u8] = bytemuck::cast_slice(input);

        let (input_buffer, output_buffer, bind_group) = self.setup(input_bytes);

        // use an encoder to define the steps that will be run
        let mut encoder = self.device.create_command_encoder(&Default::default());
        // step 1: run the compute pass (specifics defined here)
        {
            let mut compute_pass = encoder.begin_compute_pass(&Default::default());
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(input.len() as u32, 1, 1);
        }
        // step 2: copy the results to the output buffer
        encoder.copy_buffer_to_buffer(
            &input_buffer,
            0,
            &output_buffer,
            0,
            input_bytes.len() as u64,
        );

        // submit the encoder into the queue to start performing the steps
        self.queue.submit(Some(encoder.finish()));

        // a number of things are necessary to ensure that the steps were done, and I'm not sure why
        let buffer_slice = output_buffer.slice(..);
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel(); // honestly no idea
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap()); // map the output to something readable? not sure
        self.device.poll(wgpu::Maintain::Wait); // wait for everything to be done
        receiver.receive().await.unwrap().unwrap(); // check that the work was successfu. unwrapping because it's just for testing

        let output_bytes = buffer_slice.get_mapped_range();
        let output = bytemuck::cast_slice(&output_bytes);

        output.to_vec()
    }

    fn setup(&self, input_bytes: &[u8]) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
        let input_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: input_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: input_bytes.len() as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: input_buffer.as_entire_binding(),
            }],
        });
        (input_buffer, output_buffer, bind_group)
    }
}
