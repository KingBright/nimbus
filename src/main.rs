use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use winit::{
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod config;
mod llama_engine;
mod memory;
mod renderer;
mod think_filter;

use config::AppConfig;
use llama_engine::LlamaEngine;
use memory::PetMemory;
use renderer::PetRenderer;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    TrayIconBuilder,
};

struct WgpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl WgpuState {
    async fn new(
        window: Arc<winit::window::Window>,
        _width: u32,
        _height: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("Failed to find an appropriate adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let alpha_mode = if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            wgpu::CompositeAlphaMode::Opaque
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let texture_desc = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("Skia Texture"),
            view_formats: &[],
        };
        let texture = device.create_texture(&texture_desc);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                r#"
                struct VertexOutput {
                    @builtin(position) clip_position: vec4<f32>,
                    @location(0) tex_coords: vec2<f32>,
                }

                @vertex
                fn vs_main(
                    @builtin(vertex_index) in_vertex_index: u32,
                ) -> VertexOutput {
                    var pos = array<vec2<f32>, 6>(
                        vec2<f32>(-1.0,  1.0),
                        vec2<f32>(-1.0, -1.0),
                        vec2<f32>( 1.0, -1.0),
                        vec2<f32>(-1.0,  1.0),
                        vec2<f32>( 1.0, -1.0),
                        vec2<f32>( 1.0,  1.0)
                    );
                    var uv = array<vec2<f32>, 6>(
                        vec2<f32>(0.0, 0.0),
                        vec2<f32>(0.0, 1.0),
                        vec2<f32>(1.0, 1.0),
                        vec2<f32>(0.0, 0.0),
                        vec2<f32>(1.0, 1.0),
                        vec2<f32>(1.0, 0.0)
                    );

                    var out: VertexOutput;
                    out.clip_position = vec4<f32>(pos[in_vertex_index], 0.0, 1.0);
                    out.tex_coords = uv[in_vertex_index];
                    return out;
                }

                @group(0) @binding(0) var t_diffuse: texture_2d<f32>;
                @group(0) @binding(1) var s_diffuse: sampler;

                @fragment
                fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
                }
            "#,
            )),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: Some("texture_bind_group_layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("texture_bind_group"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            texture,
            texture_view,
            sampler,
            bind_group,
            width,
            height,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 && (self.width != width || self.height != height) {
            self.width = width;
            self.height = height;
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);

            let texture_desc = wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some("Skia Texture"),
                view_formats: &[],
            };
            self.texture = self.device.create_texture(&texture_desc);
            self.texture_view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group_layout =
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    multisampled: false,
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                                count: None,
                            },
                        ],
                        label: Some("texture_bind_group_layout"),
                    });

            self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
                label: Some("texture_bind_group"),
            });
        }
    }

    fn render(&self, skia_data: &[u8]) -> Result<(), wgpu::SurfaceError> {
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            skia_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

fn sync_renderer_to_window(
    renderer: &mut PetRenderer,
    window: &winit::window::Window,
    font_size: f32,
) -> winit::dpi::PhysicalSize<u32> {
    let size = window.inner_size();
    renderer.scale_factor = window.scale_factor() as f32;
    renderer.resize(size.width.max(1), size.height.max(1));
    renderer.set_font_size(font_size);
    size
}

fn set_pet_pointer_from_physical(
    renderer: &mut PetRenderer,
    window_size: winit::dpi::PhysicalSize<u32>,
    x: f64,
    y: f64,
) {
    renderer.set_mouse_position(Some((x as f32, y as f32)));
    renderer.pointer_x = x as f32 / (window_size.width.max(1) as f32 / 24.0);
    renderer.pointer_y = y as f32 / (window_size.height.max(1) as f32 / 24.0);
}

// User Activity Observer: Runs AppleScript to fetch frontmost app name and current hour
async fn observe_user_activity() -> (String, u32) {
    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first process whose frontmost is true")
        .arg("-e")
        .arg("get hours of (current date)")
        .output()
        .await;

    match output {
        Ok(out) => {
            let combined = String::from_utf8_lossy(&out.stdout).to_string();
            let mut lines = combined.lines();
            let app_name = lines.next().unwrap_or("未知应用").trim().to_string();
            let hour = lines
                .next()
                .unwrap_or("12")
                .trim()
                .parse::<u32>()
                .unwrap_or(12);
            (app_name, hour)
        }
        Err(_) => ("未知应用".to_string(), 12),
    }
}

// Pseudo-random index selector without rand dependency
fn get_pseudo_random_index(max: usize) -> usize {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    (now % max as u128) as usize
}

struct StyleSpeechProfile {
    startup_line: &'static str,
    switch_line: &'static str,
    touch_note: &'static str,
    touch_lines: &'static [&'static str],
    sleep_line: &'static str,
    work_line: &'static str,
    browser_line: &'static str,
    chat_line: &'static str,
    generic_lines: &'static [&'static str],
}

const NIMBUS_TOUCH_LINES: &[&str] = &[
    "哎呀，摸到我啦~",
    "软软地蹭一下你。",
    "小云朵被摸得轻轻晃了一下。",
];
const NIMBUS_GENERIC_LINES: &[&str] = &[
    "云朵今天也想飞得更高呢~",
    "我在空气里轻轻漂着。",
    "软软打个滚，像一小团棉花糖。",
];

const MOCHI_TOUCH_LINES: &[&str] = &[
    "喵呜，摸到耳朵边啦~",
    "我轻轻甩了甩尾巴。",
    "爪爪收起来，乖乖让你摸。",
];
const MOCHI_GENERIC_LINES: &[&str] = &[
    "我舔舔爪子，安静陪你。",
    "尾巴轻轻摇了一下。",
    "喵呜，今天也想靠近一点。",
];

const STARLIT_TOUCH_LINES: &[&str] = &[
    "咕噜噜，触角轻轻卷起来。",
    "星光皇冠晃了一下。",
    "我吐出一颗小小星泡。",
];
const STARLIT_GENERIC_LINES: &[&str] = &[
    "触角慢慢摆动，星光闪了一下。",
    "咕噜噜，我在屏幕边漂浮。",
    "一颗小星泡轻轻升起来。",
];

const CRAB_TOUCH_LINES: &[&str] = &[
    "咔嚓，摸到我的小壳啦~",
    "大螯轻轻收起来，不夹你。",
    "小螃蟹吐了个泡泡。",
];
const CRAB_GENERIC_LINES: &[&str] = &[
    "我横着走两步，悄悄举起大螯。",
    "咔嚓咔嚓，今天也精神满满。",
    "小泡泡从蟹壳边冒出来。",
];

fn style_speech_profile(pet_style: &str) -> StyleSpeechProfile {
    match pet_style {
        "mochi" => StyleSpeechProfile {
            startup_line: "抹茶小猫伸了个懒腰，耳朵轻轻竖起来。",
            switch_line: "外观已切到抹茶小猫，耳朵轻轻竖起来。",
            touch_note: "我会记住对方触碰猫耳和爪爪时更喜欢柔和、轻一点的反应。",
            touch_lines: MOCHI_TOUCH_LINES,
            sleep_line: "夜深了，小猫把尾巴圈起来，安静陪着你。",
            work_line: "我收起爪爪，趴在一旁陪你写代码。",
            browser_line: "我眨眨眼，提醒你让眼睛歇一下。",
            chat_line: "我轻轻摇尾巴，等你聊完。",
            generic_lines: MOCHI_GENERIC_LINES,
        },
        "starlit" => StyleSpeechProfile {
            startup_line: "星光章鱼慢慢醒来，触角舒展开。",
            switch_line: "外观已切到星光章鱼，触角慢慢舒展开。",
            touch_note: "我会记住对方触碰触角时更喜欢柔和、轻一点的反应。",
            touch_lines: STARLIT_TOUCH_LINES,
            sleep_line: "夜深了，星光章鱼抱住小星泡，安静陪着你。",
            work_line: "触角轻轻摆动，我在一旁陪你写代码。",
            browser_line: "我吐出一颗星泡，提醒你让眼睛歇一下。",
            chat_line: "触角慢慢晃着，我安静等你聊完。",
            generic_lines: STARLIT_GENERIC_LINES,
        },
        "crab" => StyleSpeechProfile {
            startup_line: "小螃蟹活动一下大螯，安静上线。",
            switch_line: "外观已切到红螯小螃蟹，大螯咔嚓轻响。",
            touch_note: "我会记住对方触碰蟹壳和大螯时更喜欢柔和、轻一点的反应。",
            touch_lines: CRAB_TOUCH_LINES,
            sleep_line: "夜深了，小螃蟹把大螯收近一点，安静陪着你。",
            work_line: "咔嚓轻响，我横着挪到一旁陪你写代码。",
            browser_line: "我吐出一个小泡泡，提醒你让眼睛歇一下。",
            chat_line: "小螃蟹晃晃大螯，安静等你聊完。",
            generic_lines: CRAB_GENERIC_LINES,
        },
        _ => StyleSpeechProfile {
            startup_line: "小云朵轻轻醒来，身体慢慢漂起来。",
            switch_line: "外观已切到粉蓝小云朵，身体轻轻漂起来。",
            touch_note: "我会记住对方触碰云朵身体时更喜欢柔和、轻一点的反应。",
            touch_lines: NIMBUS_TOUCH_LINES,
            sleep_line: "夜深了，小云朵收起光边，安静陪着你。",
            work_line: "我轻轻漂在一旁，陪你写代码。",
            browser_line: "我吹来一点小风，提醒你让眼睛歇一下。",
            chat_line: "我在旁边轻轻漂着，等你聊完。",
            generic_lines: NIMBUS_GENERIC_LINES,
        },
    }
}

fn startup_fallback_line(pet_style: &str) -> &'static str {
    style_speech_profile(pet_style).startup_line
}

fn startup_style_context(pet_style: &str, weather_mode: &str) -> String {
    let body = match pet_style {
        "mochi" => "当前外观是抹茶小猫；用猫耳、爪爪、尾巴或轻轻醒来的动作回应。",
        "starlit" => "当前外观是星光章鱼；用触角、星泡、皇冠或漂浮醒来的动作回应。",
        "crab" => "当前外观是红螯小螃蟹；用蟹壳、大螯、横着挪步或吐泡泡的动作回应。",
        _ => "当前外观是粉蓝小云朵；用漂浮、软软醒来或轻轻晃动的动作回应。",
    };
    format!("{body} 当前天气模式是 {weather_mode}。")
}

fn build_startup_prompt(memory_desc: &str, pet_style: &str, weather_mode: &str) -> String {
    let startup_context = startup_style_context(pet_style, weather_mode);
    let memory_desc = sanitize_startup_memory_desc(memory_desc);
    format!(
        "{} {} 你刚刚随桌宠窗口启动，正在醒来。开场像自然醒来的片刻，以身体动作、周围观察或很短的情绪为主；不像自我介绍，也不提名字。只说台词本身，最多十八个字。",
        startup_context, memory_desc
    )
}

fn sanitize_startup_memory_desc(memory_desc: &str) -> String {
    memory_desc
        .replace("Nimbus", "旧称呼")
        .replace("NIMBUS", "旧称呼")
        .replace("nimbus", "旧称呼")
        .replace("灵霓", "旧称呼")
}

fn clean_startup_line(raw: &str, pet_style: &str) -> String {
    raw.lines()
        .map(|line| {
            line.trim()
                .trim_matches(['"', '“', '”', '\'', '。', '，', ',', ' ', '　'])
        })
        .filter(|line| !line.is_empty())
        .filter(|line| !startup_line_is_unsuitable(line, pet_style))
        .map(|line| line.chars().take(24).collect::<String>())
        .next()
        .unwrap_or_else(|| startup_fallback_line(pet_style).to_string())
}

fn startup_line_is_unsuitable(text: &str, pet_style: &str) -> bool {
    text.contains("动作")
        || text.contains("台词")
        || text.contains("设定")
        || contains_legacy_pet_name(text)
        || is_startup_self_intro(text)
        || style_identity_leaks(text, pet_style)
}

fn contains_legacy_pet_name(text: &str) -> bool {
    text.to_lowercase().contains("nimbus") || text.contains("灵霓")
}

fn pet_name_prompt_suffix(pet_name: &str) -> String {
    let pet_name = pet_name.trim();
    if pet_name.is_empty() || contains_legacy_pet_name(pet_name) {
        String::new()
    } else {
        format!("，名字叫{pet_name}")
    }
}

fn is_startup_self_intro(text: &str) -> bool {
    let key: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    if key.starts_with("我是被") {
        return false;
    }
    key.starts_with("我是")
        || key.starts_with("我叫")
        || key.contains("我是桌宠")
        || key.contains("我的名字")
        || key.contains("名字是")
        || key.contains("叫我")
}

fn style_identity_leaks(text: &str, pet_style: &str) -> bool {
    let t = text.to_lowercase();
    match pet_style {
        "crab" => t.contains("云") || t.contains("猫") || t.contains("喵") || t.contains("章鱼") || t.contains("触角") || t.contains("触手"),
        "mochi" => t.contains("螃蟹") || t.contains("螯") || t.contains("云") || t.contains("章鱼") || t.contains("触角") || t.contains("触手"),
        "starlit" => t.contains("螃蟹") || t.contains("螯") || t.contains("猫") || t.contains("喵") || t.contains("云"),
        _ => t.contains("螃蟹") || t.contains("螯") || t.contains("猫") || t.contains("喵") || t.contains("章鱼") || t.contains("触角") || t.contains("触手"),
    }
}

fn clean_mutter_line(raw: &str, pet_style: &str, fallback: &str) -> String {
    raw.lines()
        .map(|line| {
            line.trim()
                .trim_matches(['"', '“', '”', '\'', '。', '，', ',', ' ', '　'])
        })
        .filter(|line| !line.is_empty())
        .filter(|line| !mutter_line_is_unsuitable(line, pet_style))
        .map(|line| line.chars().take(24).collect::<String>())
        .next()
        .unwrap_or_else(|| fallback.to_string())
}

fn mutter_line_is_unsuitable(text: &str, pet_style: &str) -> bool {
    text.contains("动作")
        || text.contains("台词")
        || text.contains("设定")
        || text.contains("提示词")
        || text.contains("回复")
        || contains_legacy_pet_name(text)
        || is_startup_self_intro(text)
        || style_identity_leaks(text, pet_style)
        || has_too_many_english_or_repeats(text)
}

fn has_too_many_english_or_repeats(text: &str) -> bool {
    if text.contains('/') || text.contains('\\') {
        return true;
    }
    let text_lower = text.to_lowercase();
    let repeats = ["hi", "hello", "crunch", "嗝", "嘟", "哈"];
    for r in repeats {
        if text_lower.matches(r).count() >= 2 {
            return true;
        }
    }
    let alpha_count = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let chinese_count = text.chars().filter(|c| (0x4e00..=0x9fa5).contains(&(*c as u32))).count();
    if alpha_count > 0 && chinese_count == 0 {
        return true;
    }
    false
}


fn style_switch_line(pet_style: &str) -> &'static str {
    style_speech_profile(pet_style).switch_line
}

fn touch_response_lines(pet_style: &str) -> &'static [&'static str] {
    style_speech_profile(pet_style).touch_lines
}

fn touch_memory_note(pet_style: &str) -> &'static str {
    style_speech_profile(pet_style).touch_note
}

fn weather_feedback_line(pet_style: &str, weather_mode: &str) -> &'static str {
    match (pet_style, weather_mode) {
        ("crab", "sunny") => "阳光照在蟹壳上，大螯亮了一下。",
        ("crab", "windy") => "风有点大，我把大螯贴近身体。",
        ("crab", "rainy") => "雨声轻轻落下，我吐出一串小泡泡。",
        ("crab", "snowy") => "雪花落下来，我把小脚缩近一点。",
        ("mochi", "sunny") => "阳光暖暖的，我把尾巴晒得软软的。",
        ("mochi", "windy") => "风吹乱了耳朵，我轻轻甩甩头。",
        ("mochi", "rainy") => "雨声细细的，我把爪爪收起来。",
        ("mochi", "snowy") => "雪花落下来，我把尾巴圈紧一点。",
        ("starlit", "sunny") => "阳光穿过星泡，触角亮了一下。",
        ("starlit", "windy") => "风把星泡吹远了，我慢慢漂回来。",
        ("starlit", "rainy") => "雨点敲着星泡，咕噜噜地响。",
        ("starlit", "snowy") => "雪花绕着皇冠转了一圈。",
        (_, "windy") => "风吹过来，我轻轻晃了一下。",
        (_, "rainy") => "雨声细细落下，我安静陪着你。",
        (_, "snowy") => "雪花慢慢飘下来了，空气变得很轻。",
        _ => "晴光落下来，心情也亮了一点。",
    }
}

// Offline muttering template based on active app and local time.
// This is only used when the model cannot answer, so it must stay style-aware.
fn get_offline_mutter(app: &str, hour: u32, pet_style: &str) -> String {
    let profile = style_speech_profile(pet_style);
    if hour >= 23 || hour <= 5 {
        return profile.sleep_line.to_string();
    }

    let app_lower = app.to_lowercase();
    if app_lower.contains("code")
        || app_lower.contains("terminal")
        || app_lower.contains("xcode")
        || app_lower.contains("cargo")
        || app_lower.contains("idea")
    {
        return profile.work_line.to_string();
    }
    if app_lower.contains("safari")
        || app_lower.contains("chrome")
        || app_lower.contains("firefox")
        || app_lower.contains("browser")
    {
        return profile.browser_line.to_string();
    }
    if app_lower.contains("chat")
        || app_lower.contains("wechat")
        || app_lower.contains("slack")
        || app_lower.contains("discord")
        || app_lower.contains("ding")
    {
        return profile.chat_line.to_string();
    }

    let idx = get_pseudo_random_index(profile.generic_lines.len());
    profile.generic_lines[idx].to_string()
}

fn create_tray_icon() -> tray_icon::Icon {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let idx = (y * 16 + x) * 4;
            // Draw a cute cloud shape
            let dx = x as f32 - 8.0;
            let dy = y as f32 - 8.0;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 5.0 || ((7..=11).contains(&y) && (3..=13).contains(&x)) {
                rgba[idx] = 135;
                rgba[idx + 1] = 206;
                rgba[idx + 2] = 250;
                rgba[idx + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, 16, 16).unwrap()
}

fn get_persona_prompt(
    persona: &str,
    pet_style: &str,
    pet_name: &str,
    weather_mode: &str,
) -> String {
    let style_desc = match pet_style {
        "mochi" => "你当前的外观是一只软萌的抹茶小猫，耳朵尖尖，适合用喵呜、舔爪、摇尾巴这类身体经验来回应。".to_string(),
        "starlit" => "你当前的外观是一只梦幻的紫色太空章鱼，头上戴着星光皇冠，适合用触角、星泡、漂浮和闪烁星光这类身体经验来回应。".to_string(),
        "crab" => "你当前的外观是一只神气活现的红色小螃蟹，长着一对大螯钳，适合用咔嚓轻响、挥动大螯、横着走路、吐泡泡这类身体经验来回应；如果旧记忆里出现其他旧外观称呼，把它理解为过去造型，不主动沿用。".to_string(),
        "nimbus" => "你当前的外观是一只雪白蓬松的云朵精灵，像一团棉花糖一样在空中漂浮，适合用轻飘飘、软绵绵变形、小雨滴或闪电这类身体经验来回应。".to_string(),
        _ => "你当前的外观是一只可爱的桌宠。".to_string(),
    };
    let name_suffix = pet_name_prompt_suffix(pet_name);

    let base_desc = match persona {
        "tsundere" => format!("你是一只傲娇性格的桌宠{name_suffix}。说话可以带一点'哼'和嘴硬感，关心对方但不直接承认，语气别扭但有温度。"),
        "toxic" => format!("你是一只毒舌性格的桌宠{name_suffix}。喜欢一针见血地吐槽日常，语气像冷幽默而不是攻击，内心仍然在陪伴对方。"),
        "moe" => format!("你是一只软萌粘人、充满治愈感的桌宠{name_suffix}。说话甜而轻，多用柔软的小动作表达依赖和陪伴。"),
        _ => format!("你是一只可爱的桌宠{name_suffix}。性格软萌活泼，说话温柔体贴，充满好奇心和活力。"),
    };

    let weather_desc = match weather_mode {
        "sunny" => "☀️晴朗舒适，阳光明媚，周围带着温暖的光晕，让人感到温暖和惬意。",
        "windy" => "💨狂风大作，气流卷动，周围刮着凉风，你可能会在风中摇曳或凌乱。",
        "rainy" => "🌧️细雨绵绵，雨滴轻轻滑落，空气潮湿而温柔。",
        "snowy" => "❄️晶莹剔透的雪花正缓缓旋转飘落，天寒地冻，适合把动作放轻一点。",
        _ => "☀️晴天",
    };

    format!("{} {} 当前环境天气是：[{}]。请结合当前外观、性格和天气，直接说一句自然中文台词，优先写具体身体动作、观察或情绪，避免解释设定或提到提示词。", base_desc, style_desc, weather_desc)
}

fn prompt_for_pet_name(current_name: &str) -> Option<String> {
    let script = format!(
        "display dialog \"请输入桌宠的新名字:\" default answer \"{}\" with title \"设置桌宠名字\" with icon note",
        current_name
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(pos) = stdout.find("text returned:") {
            let name = stdout[pos + "text returned:".len()..].trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn show_system_dialog(title: &str, message: &str) {
    let script = format!(
        "display dialog \"{}\" with title \"{}\" buttons {{\"好的\"}} default button \"好的\" with icon note",
        message, title
    );
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // 1. Load config
    let mut config = AppConfig::load_or_create();

    // 1.5 Load memory diary
    let memory = Arc::new(std::sync::Mutex::new(PetMemory::load_or_create()));

    // 2. Setup Winit Event Loop (must initialize before tray-icon on macOS)
    let event_loop = EventLoop::new()?;

    // Build System Tray
    let _tray_icon = {
        let menu = Menu::new();

        let mutter_check = CheckMenuItem::with_id(
            "mutter_enabled",
            "开启自言自语",
            true,
            config.mutter_enabled,
            None,
        );
        menu.append(&mutter_check).unwrap();

        let persona_submenu = Submenu::with_id("persona", "切换桌宠人设", true);
        let p_default = MenuItem::with_id("persona_default", "软萌活泼 (默认)", true, None);
        let p_tsundere = MenuItem::with_id("persona_tsundere", "傲娇嘴硬", true, None);
        let p_toxic = MenuItem::with_id("persona_toxic", "冷酷毒舌", true, None);
        let p_moe = MenuItem::with_id("persona_moe", "极度粘人", true, None);
        persona_submenu
            .append_items(&[&p_default, &p_tsundere, &p_toxic, &p_moe])
            .unwrap();
        menu.append(&persona_submenu).unwrap();

        let style_submenu = Submenu::with_id("pet_style", "切换桌宠外观", true);
        let s_nimbus = MenuItem::with_id("style_nimbus", "粉蓝小云朵 (Nimbus)", true, None);
        let s_mochi = MenuItem::with_id("style_mochi", "抹茶猫咪麻薯 (Mochi)", true, None);
        let s_starlit = MenuItem::with_id("style_starlit", "太空章鱼 (Starlit)", true, None);
        let s_crab = MenuItem::with_id("style_crab", "红螯小螃蟹 (Crab)", true, None);
        style_submenu
            .append_items(&[&s_nimbus, &s_mochi, &s_starlit, &s_crab])
            .unwrap();
        menu.append(&style_submenu).unwrap();

        let weather_submenu = Submenu::with_id("weather_sub", "切换桌宠天气", true);
        let w_sunny = MenuItem::with_id("weather_sunny", "☀️ 晴天", true, None);
        let w_windy = MenuItem::with_id("weather_windy", "💨 狂风", true, None);
        let w_rainy = MenuItem::with_id("weather_rainy", "🌧️ 细雨", true, None);
        let w_snowy = MenuItem::with_id("weather_snowy", "❄️ 飘雪", true, None);
        weather_submenu
            .append_items(&[&w_sunny, &w_windy, &w_rainy, &w_snowy])
            .unwrap();
        menu.append(&weather_submenu).unwrap();

        let font_submenu = Submenu::with_id("font_size", "调整字号大小", true);
        let f_20 = MenuItem::with_id("font_20", "20 像素", true, None);
        let f_24 = MenuItem::with_id("font_24", "24 像素 (默认)", true, None);
        let f_28 = MenuItem::with_id("font_28", "28 像素", true, None);
        let f_32 = MenuItem::with_id("font_32", "32 像素", true, None);
        font_submenu
            .append_items(&[&f_20, &f_24, &f_28, &f_32])
            .unwrap();
        menu.append(&font_submenu).unwrap();

        let size_submenu = Submenu::with_id("pet_size_sub", "调整桌宠大小", true);
        let s_150 = MenuItem::with_id("size_150", "小 (150 像素)", true, None);
        let s_200 = MenuItem::with_id("size_200", "中 (200 像素)", true, None);
        let s_250 = MenuItem::with_id("size_250", "大 (250 像素)", true, None);
        let s_300 = MenuItem::with_id("size_300", "特大 (300 像素)", true, None);
        size_submenu
            .append_items(&[&s_150, &s_200, &s_250, &s_300])
            .unwrap();
        menu.append(&size_submenu).unwrap();

        let config_submenu = Submenu::with_id("config_sub", "系统参数设置", true);
        let c_clear = MenuItem::with_id("action_clear", "清空对话历史", true, None);
        let c_sleep_30 = MenuItem::with_id("sleep_30", "显存休眠 30 秒", true, None);
        let c_sleep_60 = MenuItem::with_id("sleep_60", "显存休眠 60 秒", true, None);
        let c_sleep_120 = MenuItem::with_id("sleep_120", "显存休眠 120 秒", true, None);
        config_submenu
            .append_items(&[&c_clear, &c_sleep_30, &c_sleep_60, &c_sleep_120])
            .unwrap();
        menu.append(&config_submenu).unwrap();

        let rename_item = MenuItem::with_id("action_rename", "🖊️ 修改桌宠名字", true, None);
        menu.append(&rename_item).unwrap();

        let memory_submenu = Submenu::with_id("memory_sub", "💾 主人记忆管理", true);
        let m_show = MenuItem::with_id("memory_show_info", "查看记忆详情", true, None);
        let m_open = MenuItem::with_id("memory_open_dir", "打开记忆目录", true, None);
        let m_clear = MenuItem::with_id("memory_clear", "清空所有记忆", true, None);
        memory_submenu
            .append_items(&[&m_show, &m_open, &m_clear])
            .unwrap();
        menu.append(&memory_submenu).unwrap();

        menu.append(&PredefinedMenuItem::separator()).unwrap();
        let exit_item = MenuItem::with_id("action_exit", "退出桌宠", true, None);
        menu.append(&exit_item).unwrap();

        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("灵霓 Nimbus 桌宠控制面板")
            .with_icon(create_tray_icon())
            .build()
            .unwrap()
    };

    // Pet Window Logical Dimension (tight wrapping)
    let pet_win_width = config.pet_size;
    let pet_win_height = (config.pet_size as f32 * 0.55) as u32;

    // Bubble Window Initial Logical Dimension (will dynamically adapt)
    let bubble_win_width = 300;
    let bubble_win_height = 200;

    // 3. Setup Window (Pet Window)
    let window = WindowBuilder::new()
        .with_title("Nimbus Desk Pet Widget")
        .with_decorations(false) // Borderless
        .with_transparent(true) // Transparent background
        .with_resizable(false) // Lock size
        .with_window_level(if config.always_on_top {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        })
        .with_inner_size(winit::dpi::LogicalSize::new(
            pet_win_width as f64,
            pet_win_height as f64,
        ))
        .build(&event_loop)?;

    // Setup Bubble Window (Click-through, hidden initially)
    let bubble_window = WindowBuilder::new()
        .with_title("Nimbus Desk Pet Bubble")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_window_level(if config.always_on_top {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        })
        .with_inner_size(winit::dpi::LogicalSize::new(
            bubble_win_width as f64,
            bubble_win_height as f64,
        ))
        .with_visible(false)
        .build(&event_loop)?;

    if let Err(err) = bubble_window.set_cursor_hittest(false) {
        eprintln!("Failed to enable bubble click-through: {err:?}");
    }

    // Position windows in bottom right corner by default
    if let Some(monitor) = window.current_monitor() {
        let screen_size = monitor.size();
        let scale_factor = window.scale_factor();
        let x = screen_size.width - (pet_win_width as f64 * scale_factor) as u32 - 50;
        let y = screen_size.height - (pet_win_height as f64 * scale_factor) as u32 - 100;
        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }

    let window = Arc::new(window);
    let window_clone = window.clone();
    let bubble_window = Arc::new(bubble_window);
    let bubble_window_clone = bubble_window.clone();

    // 3. Initialize Wgpu States
    let mut wgpu_state = WgpuState::new(window.clone(), pet_win_width, pet_win_height).await?;
    let mut bubble_wgpu_state =
        WgpuState::new(bubble_window.clone(), bubble_win_width, bubble_win_height).await?;

    // 4. Initialize custom 2D Renderers
    let pet_initial_size = window.inner_size();
    let bubble_initial_size = bubble_window.inner_size();

    let mut pet_renderer = PetRenderer::new(
        pet_initial_size.width.max(1),
        pet_initial_size.height.max(1),
    );
    pet_renderer.draw_bubble_enabled = false;
    pet_renderer.draw_pet_enabled = true;
    pet_renderer.active_style = config.pet_style.clone();
    pet_renderer.active_weather = config.weather_mode.clone();
    sync_renderer_to_window(&mut pet_renderer, &window, config.font_size);

    let mut bubble_renderer = PetRenderer::new(
        bubble_initial_size.width.max(1),
        bubble_initial_size.height.max(1),
    );
    bubble_renderer.draw_bubble_enabled = true;
    bubble_renderer.draw_pet_enabled = false;
    bubble_renderer.active_style = config.pet_style.clone();
    bubble_renderer.active_weather = config.weather_mode.clone();
    sync_renderer_to_window(&mut bubble_renderer, &bubble_window, config.font_size);

    // 5. Initialize in-process LlamaEngine with Smart Idle Offload config
    let llama_engine = if let Some(model_path) = &config.model_path {
        match LlamaEngine::new(model_path.clone(), config.idle_offload_secs) {
            Ok(engine) => {
                println!("LlamaEngine initialized successfully (In-Process mode)!");
                let info = engine.resource_info();
                println!(
                    "[Nimbus] Model resource baseline: path={}, file={} bytes, online={}, active_context={} bytes",
                    info.model_path.display(),
                    info.model_file_bytes,
                    info.model_online,
                    info.active_metal_context_bytes
                );
                Some(Arc::new(engine))
            }
            Err(e) => {
                eprintln!("Failed to initialize in-process LlamaEngine: {:?}", e);
                None
            }
        }
    } else {
        println!("No local model configured in config.toml. Running in GUI demonstration mode.");
        None
    };

    // Shared State for User Activity Observer (AppName, LocalHour)
    let observer_state = Arc::new(std::sync::Mutex::new(("未知应用".to_string(), 12u32)));
    let observer_state_clone = observer_state.clone();

    // Spawn Background Activity Observer Task (Every 45 seconds)
    let memory_clone = memory.clone();
    tokio::spawn(async move {
        loop {
            let (app, hour) = observe_user_activity().await;
            if let Ok(mut guard) = observer_state_clone.lock() {
                *guard = (app.clone(), hour);
            }
            if let Ok(mut mem) = memory_clone.lock() {
                mem.record_app(&app, hour);
            }
            tokio::time::sleep(std::time::Duration::from_secs(45)).await;
        }
    });

    // Interaction & Timer states
    let mut cursor_position = winit::dpi::PhysicalPosition::new(0.0, 0.0);
    let mut last_interactive_time = std::time::Instant::now();
    let mut last_hover_trigger_time: Option<std::time::Instant> = None;
    let mut was_pointer_on_pet = false;
    let mut was_online = false;
    let mut is_pointer_on_pet = false;
    let mut surprised_timer: Option<std::time::Instant> = None;
    let mut is_muttering = false;
    let mut last_mouse_phys_x = 0.0f64;
    let mut last_mouse_phys_y = 0.0f64;

    // Channels for text streaming and terminal input
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(100);
    let mut active_response = String::new();
    let mut is_startup_generation = false;

    // 6. Spawn Stdin non-blocking reader task
    let idle_sec_hint = config.idle_offload_secs;
    let pet_name_clone = config.pet_name.clone();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut line = String::new();

        println!("==================================================");
        println!("      桌宠 {} 极速推理与资源优化版       ", pet_name_clone);
        println!("==================================================");
        println!("* 一体化单进程推理已加载。");
        println!("* 闲置 {} 秒将自动释放 GPU 显存 / 内存。", idle_sec_hint);
        println!("* 在此终端输入文字按 [回车] 即可与屏幕桌宠聊天。");
        println!("* 输入 /clear 清空上下文，输入 /exit 退出程序。");
        println!("==================================================");

        loop {
            line.clear();
            print!("\n>> ");
            use std::io::Write;
            let _ = std::io::stdout().flush();

            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        let _ = stdin_tx.send(trimmed.to_string()).await;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading stdin: {:?}", e);
                    break;
                }
            }
        }
    });

    // Redraw rate limiting states
    let mut last_draw_time = std::time::Instant::now();
    let mut bubble_timer: Option<std::time::Instant> = None;
    let mut bubble_show_duration = std::time::Duration::from_secs(config.bubble_timeout_secs);

    // Thinking Block Stream Filter states
    let mut filter = think_filter::ThinkBlockFilter::new(config.expose_think, false);
    let mut in_thinking_block = false;

    if let Some(ref engine) = llama_engine {
        filter = think_filter::ThinkBlockFilter::new(config.expose_think, false);
        active_response.clear();
        is_muttering = true;
        is_startup_generation = true;

        let memory_desc = memory
            .lock()
            .map(|mem| mem.describe_for_prompt())
            .unwrap_or_else(|_| "你还没有关于屏幕前的人的日常偏好记忆。".to_string());
        let prompt = build_startup_prompt(&memory_desc, &config.pet_style, &config.weather_mode);
        let engine_clone = engine.clone();
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            if let Ok(mut stream) = engine_clone.generate_mutter_stream(&prompt).await {
                while let Some(chunk) = stream.next().await {
                    let _ = tx_clone.send(chunk).await;
                }
                let _ = tx_clone.send("[DONE]".to_string()).await;
            } else {
                let _ = tx_clone.send("[OFFLINE]".to_string()).await;
            }
        });
    } else {
        bubble_renderer.set_text(startup_fallback_line(&config.pet_style));
        bubble_timer = Some(std::time::Instant::now());
    }

    // Trigger initial redraw
    window_clone.request_redraw();
    bubble_window_clone.request_redraw();

    let window_for_run = window_clone.clone();

    // 7. Run Event Loop
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);

        // Detect LLM Offload/Restore status changes
        let is_online = if let Some(ref engine) = llama_engine {
            engine.is_model_online()
        } else {
            false
        };

        if was_online && !is_online {
            // Smart Offload is an internal resource event; it should not steal the pet's speech.
            pet_renderer.pet_state = "sleeping".to_string();
            last_interactive_time = std::time::Instant::now();
            window_for_run.request_redraw();
        } else if !was_online && is_online {
            // Model re-activated
            if pet_renderer.pet_state == "sleeping" {
                pet_renderer.pet_state = "idle".to_string();
                window_for_run.request_redraw();
            }
        }
        was_online = is_online;

        // Process incoming Stdin commands
        while let Ok(input_prompt) = stdin_rx.try_recv() {
            if input_prompt == "/exit" {
                println!("Exiting Nimbus Desk Pet. Goodbye!");
                elwt.exit();
                return;
            }
            if input_prompt == "/clear" {
                if let Some(ref engine) = llama_engine {
                    let _ = engine.clear_history();
                }
                active_response.clear();
                bubble_renderer.set_text("✨ 对话上下文已重置！");
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(6);
                last_interactive_time = std::time::Instant::now();
                window_for_run.request_redraw();
                println!("\n>> 历史已重置。");
                continue;
            }

            if let Some(ref engine) = llama_engine {
                is_startup_generation = false;
                filter = think_filter::ThinkBlockFilter::new(config.expose_think, false);
                in_thinking_block = false;
                active_response.clear();
                pet_renderer.pet_state = "thinking".to_string();
                bubble_renderer.set_text("思考中...");
                bubble_timer = None; // Disable hide timer during generation
                last_interactive_time = std::time::Instant::now();
                window_for_run.request_redraw();

                if let Ok(mut mem) = memory.lock() {
                    mem.increment_interactions();
                }

                let memory_desc = memory
                    .lock()
                    .map(|mem| mem.describe_for_prompt())
                    .unwrap_or_else(|_| "你还没有关于屏幕前的人的日常偏好记忆。".to_string());

                let persona_prompt = get_persona_prompt(&config.persona, &config.pet_style, &config.pet_name, &config.weather_mode);
                let prompt = format!(
                    "人设设定：{}。{}。屏幕前的人对你说：\"{}\"。",
                    persona_prompt, memory_desc, input_prompt
                );

                let engine_clone = engine.clone();
                let tx_clone = tx.clone();

                tokio::spawn(async move {
                    if let Ok(mut stream) = engine_clone.generate_stream(&prompt).await {
                        while let Some(chunk) = stream.next().await {
                            let _ = tx_clone.send(chunk).await;
                        }
                        let _ = tx_clone.send("[DONE]".to_string()).await;
                    } else {
                        let _ = tx_clone.send("[OFFLINE]".to_string()).await;
                    }
                });
            } else {
                active_response = "本地模型未加载！\n请在 config.toml 中配置 model_path。".to_string();
                bubble_renderer.set_text(&active_response);
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(6);
                window_for_run.request_redraw();
            }
        }

        // Process streamed tokens from LLM Engine (includes Chat & Mutter)
        while let Ok(new_text) = rx.try_recv() {
            if new_text == "[DONE]" {
                let outputs = filter.flush();
                for out in outputs {
                    if out.event == "think" {
                        if !in_thinking_block {
                            in_thinking_block = true;
                            active_response.push_str("[思考] ");
                        }
                        active_response.push_str(&out.content);
                    } else {
                        if in_thinking_block {
                            in_thinking_block = false;
                            active_response.push_str("\n\n");
                        }
                        active_response.push_str(&out.content);
                    }
                }
                let was_muttering = is_muttering;
                is_muttering = false;
                println!();
                println!("[LLM Output Done] is_startup: {}, was_muttering: {}, response: {:?}", is_startup_generation, was_muttering, active_response);

                if is_startup_generation {
                    active_response = clean_startup_line(&active_response, &config.pet_style);
                    println!("[LLM Startup Cleaned] response: {:?}", active_response);
                    bubble_renderer.set_text(&active_response);
                    is_startup_generation = false;
                } else if was_muttering {
                    let (app, hour) = {
                        if let Ok(guard) = observer_state.lock() {
                            guard.clone()
                        } else {
                            ("未知应用".to_string(), 12)
                        }
                    };
                    let fallback = get_offline_mutter(&app, hour, &config.pet_style);
                    active_response = clean_mutter_line(&active_response, &config.pet_style, &fallback);
                    println!("[LLM Mutter Cleaned] response: {:?}", active_response);
                    bubble_renderer.set_text(&active_response);
                } else {
                    bubble_renderer.set_text(&active_response);
                }

                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(config.bubble_timeout_secs);
                last_interactive_time = std::time::Instant::now();
                if !active_response.trim().is_empty() {
                    if let Ok(mut mem) = memory.lock() {
                        mem.record_spoken_line(&active_response);
                    }
                }
                window_for_run.request_redraw();
            } else if new_text == "[OFFLINE]" {
                if is_startup_generation {
                    active_response = startup_fallback_line(&config.pet_style).to_string();
                    bubble_renderer.set_text(&active_response);
                    is_startup_generation = false;
                    is_muttering = false;
                    bubble_timer = Some(std::time::Instant::now());
                    bubble_show_duration =
                        std::time::Duration::from_secs(config.bubble_timeout_secs);
                    window_for_run.request_redraw();
                    continue;
                }
                // Mutter fallback triggered if model is currently offline (handled below in muttering dispatch)
                let (app, hour) = {
                    if let Ok(guard) = observer_state.lock() {
                        guard.clone()
                    } else {
                        ("未知应用".to_string(), 12)
                    }
                };
                let mutter = get_offline_mutter(&app, hour, &config.pet_style);
                bubble_renderer.set_text(&mutter);
                is_muttering = false;
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(8);
                last_interactive_time = std::time::Instant::now();
                window_for_run.request_redraw();
            } else {
                if !is_startup_generation {
                    print!("{}", new_text);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
                is_muttering = true;

                let outputs = filter.feed(&new_text);
                for out in outputs {
                    if out.event == "think" {
                        if !in_thinking_block {
                            in_thinking_block = true;
                            active_response.push_str("[思考] ");
                        }
                        active_response.push_str(&out.content);
                    } else {
                        if in_thinking_block {
                            in_thinking_block = false;
                            active_response.push_str("\n\n");
                        }
                        active_response.push_str(&out.content);
                    }
                }
                if !is_startup_generation {
                    bubble_renderer.set_text(&active_response);
                    window_for_run.request_redraw();
                }
            }
        }

        // Handle auto-fade timeout for chat bubble
        if let Some(timer) = bubble_timer {
            if std::time::Instant::now().duration_since(timer) > bubble_show_duration {
                active_response.clear();
                bubble_renderer.set_text("");
                bubble_timer = None;
                window_for_run.request_redraw();
            }
        }

        // Position & Size alignment step
        let pet_scale = window_for_run.scale_factor() as f32;
        let bubble_scale = bubble_window_clone.scale_factor() as f32;

        if (pet_renderer.scale_factor - pet_scale).abs() > f32::EPSILON {
            pet_renderer.scale_factor = pet_scale;
            pet_renderer.set_font_size(config.font_size);
        }
        if (bubble_renderer.scale_factor - bubble_scale).abs() > f32::EPSILON {
            bubble_renderer.scale_factor = bubble_scale;
            bubble_renderer.set_font_size(config.font_size);
        }

        if bubble_timer.is_some() && bubble_renderer.has_text() {
            let (b_w_phys, b_h_phys) = bubble_renderer.measure_bubble_size();
            if b_w_phys > 0.0 && b_h_phys > 0.0 {
                let b_w_log = b_w_phys as f64 / bubble_scale as f64;
                let b_h_log = b_h_phys as f64 / bubble_scale as f64;

                let current_size = bubble_window_clone.inner_size();
                let mut effective_bubble_size = current_size;
                if (current_size.width as f64 - b_w_phys as f64).abs() > 1.0
                    || (current_size.height as f64 - b_h_phys as f64).abs() > 1.0 {
                    let requested = bubble_window_clone
                        .request_inner_size(winit::dpi::LogicalSize::new(b_w_log, b_h_log));
                    effective_bubble_size = requested.unwrap_or_else(|| {
                        winit::dpi::PhysicalSize::new(
                            b_w_phys.round().max(1.0) as u32,
                            b_h_phys.round().max(1.0) as u32,
                        )
                    });
                    bubble_wgpu_state.resize(
                        effective_bubble_size.width.max(1),
                        effective_bubble_size.height.max(1),
                    );
                    bubble_renderer.resize(
                        effective_bubble_size.width.max(1),
                        effective_bubble_size.height.max(1),
                    );
                }

                if let Ok(pet_pos) = window_for_run.outer_position() {
                    let pet_size = window_for_run.inner_size();

                    let bubble_y = if pet_pos.y < effective_bubble_size.height as i32 + 30 {
                        bubble_renderer.bubble_on_bottom = true;
                        pet_pos.y + pet_size.height as i32
                    } else {
                        bubble_renderer.bubble_on_bottom = false;
                        pet_pos.y - effective_bubble_size.height as i32
                    };

                    let mut bubble_x = pet_pos.x
                        + (pet_size.width as i32 - effective_bubble_size.width as i32) / 2;

                    if let Some(monitor) = window_for_run.current_monitor() {
                        let monitor_pos = monitor.position();
                        let monitor_size = monitor.size();
                        let margin = (8.0 * bubble_scale as f64).round() as i32;
                        let min_x = monitor_pos.x + margin;
                        let max_x = monitor_pos.x + monitor_size.width as i32
                            - effective_bubble_size.width as i32
                            - margin;
                        bubble_x = bubble_x.clamp(min_x, max_x.max(min_x));
                    }

                    bubble_window_clone.set_outer_position(winit::dpi::PhysicalPosition::new(
                        bubble_x, bubble_y,
                    ));
                    if !bubble_window_clone.is_visible().unwrap_or(false) {
                        bubble_window_clone.set_visible(true);
                    }
                    bubble_window_clone.request_redraw();
                }
            }
        } else {
            if bubble_window_clone.is_visible().unwrap_or(false) {
                bubble_window_clone.set_visible(false);
            }
        }

        match event {
            Event::WindowEvent { event, window_id } => {
                if window_id == window_for_run.id() {
                    match event {
                        WindowEvent::CloseRequested => {
                            elwt.exit();
                        }
                        WindowEvent::Resized(size) => {
                            wgpu_state.resize(size.width, size.height);
                            pet_renderer.scale_factor = window_for_run.scale_factor() as f32;
                            pet_renderer.resize(size.width.max(1), size.height.max(1));
                            pet_renderer.set_font_size(config.font_size);
                            window_for_run.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let size = sync_renderer_to_window(
                                &mut pet_renderer,
                                &window_for_run,
                                config.font_size,
                            );
                            wgpu_state.resize(size.width.max(1), size.height.max(1));
                            window_for_run.request_redraw();
                        }
                        WindowEvent::RedrawRequested => {
                            if !pet_renderer.pointer_inside {
                                use mouse_position::mouse_position::Mouse;
                                if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
                                    if let Ok(win_pos) = window_for_run.outer_position() {
                                        let px = x as f64;
                                        let py = y as f64;
                                        let wx = px - win_pos.x as f64;
                                        let wy = py - win_pos.y as f64;
                                        set_pet_pointer_from_physical(
                                            &mut pet_renderer,
                                            window_for_run.inner_size(),
                                            wx,
                                            wy,
                                        );
                                    }
                                }
                            }

                            // 智能状态决策引擎 (Smart State Decision Engine)
                            let current_state = {
                                let now_inst = std::time::Instant::now();
                                let is_surprised = if let Some(st) = surprised_timer {
                                    now_inst.duration_since(st) < std::time::Duration::from_millis(1500)
                                } else {
                                    false
                                };

                                if is_surprised {
                                    "surprised".to_string()
                                } else if is_pointer_on_pet {
                                    "tickled".to_string()
                                } else if is_muttering {
                                    if in_thinking_block {
                                        "thinking".to_string()
                                    } else {
                                        "muttering".to_string()
                                    }
                                } else if pet_renderer.pointer_inside {
                                    "attentive".to_string()
                                } else if !is_online {
                                    "sleeping".to_string()
                                } else {
                                    let (app, hour) = {
                                        if let Ok(guard) = observer_state.lock() {
                                            guard.clone()
                                        } else {
                                            ("未知应用".to_string(), 12)
                                        }
                                    };
                                    let idle_duration = now_inst.duration_since(last_interactive_time);

                                    if idle_duration > std::time::Duration::from_secs(90) {
                                        "bored".to_string()
                                    } else {
                                        let app_lower = app.to_lowercase();
                                        let is_dev_app = app_lower.contains("code")
                                            || app_lower.contains("terminal")
                                            || app_lower.contains("xcode")
                                            || app_lower.contains("cargo")
                                            || app_lower.contains("idea")
                                            || app_lower.contains("warp");

                                        if is_dev_app {
                                            "working".to_string()
                                        } else if hour >= 23 || hour <= 5 {
                                            "tired".to_string()
                                        } else {
                                            "idle".to_string()
                                        }
                                    }
                                }
                            };
                            pet_renderer.pet_state = current_state;

                            pet_renderer.update_and_draw();

                            let skia_data = pet_renderer.data();
                            if let Err(e) = wgpu_state.render(skia_data) {
                                eprintln!("wgpu render error: {:?}", e);
                            }
                            last_draw_time = std::time::Instant::now();
                        }
                        WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                            let is_on_pet = pet_renderer.body_hit_test(cursor_position.x, cursor_position.y).unwrap_or(false);

                            if state == ElementState::Pressed {
                                if is_on_pet {
                                    if let Err(e) = window_for_run.drag_window() {
                                        eprintln!("Failed to drag window: {:?}", e);
                                    }
                                }
                            } else if state == ElementState::Released {
                                surprised_timer = Some(std::time::Instant::now());
                                pet_renderer.add_emotion(-0.02, 0.15);
                            }
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            pet_renderer.pointer_inside = true;
                            cursor_position = position;
                            last_interactive_time = std::time::Instant::now();

                            set_pet_pointer_from_physical(
                                &mut pet_renderer,
                                window_for_run.inner_size(),
                                position.x,
                                position.y,
                            );

                            let phys_x = position.x;
                            let phys_y = position.y;
                            if last_mouse_phys_x > 0.0 && last_mouse_phys_y > 0.0 {
                                let dx = phys_x - last_mouse_phys_x;
                                let dy = phys_y - last_mouse_phys_y;
                                let speed = (dx * dx + dy * dy).sqrt();
                                if speed > 220.0 {
                                    surprised_timer = Some(std::time::Instant::now());
                                    pet_renderer.add_emotion(-0.08, 0.35);
                                }
                            }
                            last_mouse_phys_x = phys_x;
                            last_mouse_phys_y = phys_y;

                            let is_on_pet = pet_renderer.body_hit_test(position.x, position.y).unwrap_or(false);
                            is_pointer_on_pet = is_on_pet;

                            if is_on_pet && !was_pointer_on_pet {
                                let now = std::time::Instant::now();
                                let throttle = match last_hover_trigger_time {
                                    Some(t) => now.duration_since(t) > std::time::Duration::from_secs(15),
                                    None => true,
                                };

                                if throttle && pet_renderer.pet_state != "thinking" {
                                    last_hover_trigger_time = Some(now);
                                    last_interactive_time = now;
                                    pet_renderer.add_emotion(0.18, 0.08);
                                    if let Ok(mut mem) = memory.lock() {
                                        mem.increment_touch();
                                        mem.add_personality_note(touch_memory_note(&config.pet_style));
                                    }

                                    let blush_phrases = touch_response_lines(&config.pet_style);
                                    let idx = get_pseudo_random_index(blush_phrases.len());
                                    bubble_renderer.set_text(blush_phrases[idx]);

                                    bubble_timer = Some(now);
                                    bubble_show_duration = std::time::Duration::from_secs(5);
                                    window_for_run.request_redraw();
                                }
                            }
                            was_pointer_on_pet = is_on_pet;
                            window_for_run.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            pet_renderer.pointer_inside = false;
                            is_pointer_on_pet = false;
                            window_for_run.request_redraw();
                        }
                        _ => {}
                    }
                } else if window_id == bubble_window_clone.id() {
                    match event {
                        WindowEvent::Resized(size) => {
                            bubble_wgpu_state.resize(size.width.max(1), size.height.max(1));
                            bubble_renderer.scale_factor = bubble_window_clone.scale_factor() as f32;
                            bubble_renderer.resize(size.width.max(1), size.height.max(1));
                            bubble_renderer.set_font_size(config.font_size);
                            bubble_window_clone.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let size = sync_renderer_to_window(
                                &mut bubble_renderer,
                                &bubble_window_clone,
                                config.font_size,
                            );
                            bubble_wgpu_state.resize(size.width.max(1), size.height.max(1));
                            bubble_window_clone.request_redraw();
                        }
                        WindowEvent::RedrawRequested
                            if bubble_timer.is_some() && bubble_renderer.has_text() =>
                        {
                            bubble_renderer.update_and_draw();
                            let skia_data = bubble_renderer.data();
                            if let Err(e) = bubble_wgpu_state.render(skia_data) {
                                eprintln!("bubble wgpu render error: {:?}", e);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::AboutToWait => {
                let now = std::time::Instant::now();

                // Handle System Tray/Menubar events
                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    let id = event.id.0.as_str();
                    match id {
                        "mutter_enabled" => {
                            config.mutter_enabled = !config.mutter_enabled;
                            config.save();
                        }
                        "action_rename" => {
                            if let Some(new_name) = prompt_for_pet_name(&config.pet_name) {
                                config.pet_name = new_name.clone();
                                config.save();
                                bubble_renderer.set_text("名字我记住了，会慢慢适应这个称呼。");
                                bubble_timer = Some(std::time::Instant::now());
                                bubble_show_duration = std::time::Duration::from_secs(5);
                                window_for_run.request_redraw();
                                println!("\n>> 桌宠重命名为：{}", new_name);
                            }
                        }
                        "memory_show_info" => {
                            let path = PetMemory::get_path();
                            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let size_str = if size_bytes > 1024 {
                                format!("{:.2} KB", size_bytes as f64 / 1024.0)
                            } else {
                                format!("{} Bytes", size_bytes)
                            };

                            let info_msg = {
                                if let Ok(mem) = memory.lock() {
                                    let top_apps = mem.get_top_apps();
                                    let top_hours = mem.get_top_hours();
                                    let active_desc = if top_apps.is_empty() {
                                        "暂无前台应用习惯记录。".to_string()
                                    } else {
                                        format!("主人最常用的前台应用是：{}；通常活跃时间是：{}。",
                                            top_apps.join(", "),
                                            top_hours.iter().map(|h| format!("{}点", h)).collect::<Vec<_>>().join("/")
                                        )
                                    };
                                    format!("记忆文件：~/.nimbus_memory.json\\n文件大小：{}\\n互动次数：{}次\\n\\n【日常习惯习惯记忆】\\n{}",
                                        size_str, mem.total_interactions, active_desc)
                                } else {
                                    "无法读取记忆。".to_string()
                                }
                            };
                            show_system_dialog("主人记忆日记详情", &info_msg);
                        }
                        "memory_open_dir" => {
                            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                            let _ = std::process::Command::new("open").arg(&home).status();
                        }
                        "memory_clear" => {
                            if let Ok(mut mem) = memory.lock() {
                                mem.clear();
                                bubble_renderer.set_text("记忆已清空，我会重新认识你。");
                                bubble_timer = Some(std::time::Instant::now());
                                bubble_show_duration = std::time::Duration::from_secs(5);
                                window_for_run.request_redraw();
                                println!("\n>> 主人习惯记忆已重置。");
                            }
                        }
                        "persona_default" => {
                            config.persona = "default".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            bubble_renderer.set_text("语气调回柔和自然，我会继续观察你。");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "persona_tsundere" => {
                            config.persona = "tsundere".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            bubble_renderer.set_text("哼，才不是想让你陪我呢！");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "persona_toxic" => {
                            config.persona = "toxic".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            bubble_renderer.set_text("今天的工作完成了吗？");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "persona_moe" => {
                            config.persona = "moe".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            bubble_renderer.set_text("我把语气放软一点，靠近陪你。");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "style_nimbus" => {
                            config.pet_style = "nimbus".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "nimbus".to_string();
                            bubble_renderer.active_style = "nimbus".to_string();
                                bubble_renderer.set_text(style_switch_line("nimbus"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "style_mochi" => {
                            config.pet_style = "mochi".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "mochi".to_string();
                            bubble_renderer.active_style = "mochi".to_string();
                            bubble_renderer.set_text(style_switch_line("mochi"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "style_starlit" => {
                            config.pet_style = "starlit".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "starlit".to_string();
                            bubble_renderer.active_style = "starlit".to_string();
                            bubble_renderer.set_text(style_switch_line("starlit"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "style_crab" => {
                            config.pet_style = "crab".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "crab".to_string();
                            bubble_renderer.active_style = "crab".to_string();
                            bubble_renderer.set_text(style_switch_line("crab"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "weather_sunny" => {
                            config.weather_mode = "sunny".to_string();
                            config.save();
                            pet_renderer.active_weather = "sunny".to_string();
                            bubble_renderer.active_weather = "sunny".to_string();
                            bubble_renderer.set_text(weather_feedback_line(&config.pet_style, "sunny"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "weather_windy" => {
                            config.weather_mode = "windy".to_string();
                            config.save();
                            pet_renderer.active_weather = "windy".to_string();
                            bubble_renderer.active_weather = "windy".to_string();
                            bubble_renderer.set_text(weather_feedback_line(&config.pet_style, "windy"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "weather_rainy" => {
                            config.weather_mode = "rainy".to_string();
                            config.save();
                            pet_renderer.active_weather = "rainy".to_string();
                            bubble_renderer.active_weather = "rainy".to_string();
                            bubble_renderer.set_text(weather_feedback_line(&config.pet_style, "rainy"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "weather_snowy" => {
                            config.weather_mode = "snowy".to_string();
                            config.save();
                            pet_renderer.active_weather = "snowy".to_string();
                            bubble_renderer.active_weather = "snowy".to_string();
                            bubble_renderer.set_text(weather_feedback_line(&config.pet_style, "snowy"));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "size_150" | "size_200" | "size_250" | "size_300" => {
                            let new_size = match id {
                                "size_150" => 150,
                                "size_200" => 200,
                                "size_250" => 250,
                                "size_300" => 300,
                                _ => 200,
                            };
                            config.pet_size = new_size;
                            config.save();

                            let new_height = (new_size as f32 * 0.55) as u32;
                            let pet_scale = window_for_run.scale_factor();
                            let p_w_phys = (new_size as f64 * pet_scale).round() as u32;
                            let p_h_phys = (new_height as f64 * pet_scale).round() as u32;
                            let requested = window_for_run.request_inner_size(
                                winit::dpi::LogicalSize::new(new_size as f64, new_height as f64),
                            );
                            let target_size = requested.unwrap_or_else(|| {
                                winit::dpi::PhysicalSize::new(p_w_phys, p_h_phys)
                            });
                            wgpu_state.resize(target_size.width.max(1), target_size.height.max(1));
                            pet_renderer.scale_factor = pet_scale as f32;
                            pet_renderer.resize(target_size.width.max(1), target_size.height.max(1));
                            pet_renderer.set_font_size(config.font_size);

                            bubble_renderer.set_text(&format!("✨ 身体缩放完成！当前尺寸：{} 像素", new_size));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "font_20" => {
                            config.font_size = 20.0;
                            config.save();
                            pet_renderer.set_font_size(20.0);
                            bubble_renderer.set_font_size(20.0);
                            window_for_run.request_redraw();
                        }
                        "font_24" => {
                            config.font_size = 24.0;
                            config.save();
                            pet_renderer.set_font_size(24.0);
                            bubble_renderer.set_font_size(24.0);
                            window_for_run.request_redraw();
                        }
                        "font_28" => {
                            config.font_size = 28.0;
                            config.save();
                            pet_renderer.set_font_size(28.0);
                            bubble_renderer.set_font_size(28.0);
                            window_for_run.request_redraw();
                        }
                        "font_32" => {
                            config.font_size = 32.0;
                            config.save();
                            pet_renderer.set_font_size(32.0);
                            bubble_renderer.set_font_size(32.0);
                            window_for_run.request_redraw();
                        }
                        "action_clear" => {
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            active_response.clear();
                            bubble_renderer.set_text("✨ 对话上下文已重置！");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "sleep_30" => {
                            config.idle_offload_secs = 30;
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.set_idle_timeout(30);
                            }
                        }
                        "sleep_60" => {
                            config.idle_offload_secs = 60;
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.set_idle_timeout(60);
                            }
                        }
                        "sleep_120" => {
                            config.idle_offload_secs = 120;
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.set_idle_timeout(120);
                            }
                        }
                        "action_exit" => {
                            println!("Exiting Desk Pet via Menu Bar.");
                            elwt.exit();
                            return;
                        }
                        _ => {}
                    }
                }

                // Periodic Muttering: Check if idle for > 3 minutes (180 seconds)
                if config.mutter_enabled && now.duration_since(last_interactive_time) > std::time::Duration::from_secs(180) && bubble_timer.is_none() {
                    last_interactive_time = now;

                    if let Some(ref engine) = llama_engine {
                        if engine.is_model_online() {
                            let (app, hour) = {
                                if let Ok(guard) = observer_state.lock() {
                                    guard.clone()
                                } else {
                                    ("未知应用".to_string(), 12)
                                }
                            };


                            let short_persona = match config.persona.as_str() {
                                "tsundere" => "傲娇别扭，嘴硬心软",
                                "toxic" => "毒舌犀利，幽默吐槽",
                                "moe" => "极其软萌，超甜粘人",
                                _ => "活泼可爱，贴贴治愈",
                            };
                            let short_style = match config.pet_style.as_str() {
                                "mochi" => "抹茶小猫（喜欢喵喵叫，说话带‘喵~’）",
                                "starlit" => "太空章鱼（喜欢挥舞触手、咕噜噜吐泡泡）",
                                "crab" => "红色小螃蟹（喜欢横着走、剪螯发出咔嚓声）",
                                "nimbus" => "云朵精灵（喜欢在空中飘浮、说话轻飘飘）",
                                _ => "可爱的桌宠",
                            };
                            let short_weather = match config.weather_mode.as_str() {
                                "sunny" => "晴天☀️",
                                "windy" => "刮大风💨",
                                "rainy" => "下雨🌧️",
                                "snowy" => "下雪❄️",
                                _ => "晴天",
                            };
                            let prompt = format!(
                                "你是桌宠{}，性格是[{}]，外观是[{}]。主人当前在前台用[{}]，时间是{}点，你的天气是{}。请配合设定自发无聊说一句话，绝对不要有任何格式，不能有英文，不超过15个字！",
                                config.pet_name, short_persona, short_style, app, hour, short_weather
                            );

                            let engine_clone = engine.clone();
                            let tx_clone = tx.clone();

                            tokio::spawn(async move {
                                if let Ok(mut stream) = engine_clone.generate_mutter_stream(&prompt).await {
                                    while let Some(chunk) = stream.next().await {
                                        let _ = tx_clone.send(chunk).await;
                                    }
                                    let _ = tx_clone.send("[DONE]".to_string()).await;
                                } else {
                                    let _ = tx_clone.send("[OFFLINE]".to_string()).await;
                                }
                            });
                        } else {
                            let (app, hour) = {
                                if let Ok(guard) = observer_state.lock() {
                                    guard.clone()
                                } else {
                                    ("未知应用".to_string(), 12)
                                }
                            };
                            let mutter = get_offline_mutter(&app, hour, &config.pet_style);
                            bubble_renderer.set_text(&mutter);
                            bubble_timer = Some(now);
                            bubble_show_duration = std::time::Duration::from_secs(8);
                            window_for_run.request_redraw();
                        }
                    }
                }

                let is_active = pet_renderer.pointer_inside
                    || pet_renderer.pet_state == "thinking"
                    || is_muttering
                    || bubble_timer.is_some();
                let interval = if is_active {
                    std::time::Duration::from_millis(33)
                } else {
                    std::time::Duration::from_millis(125)
                };

                if now.duration_since(last_draw_time) >= interval {
                    window_for_run.request_redraw();
                    if bubble_window_clone.is_visible().unwrap_or(false) {
                        bubble_window_clone.request_redraw();
                    }
                }
                elwt.set_control_flow(ControlFlow::WaitUntil(now + interval));
            }
            _ => {}
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_no_crab_identity_leak(text: &str) {
        assert!(
            !text.contains("云朵") && !text.contains("小云") && !text.contains("猫咪"),
            "crab text leaked another identity: {text}"
        );
    }

    #[test]
    fn crab_local_visible_text_stays_crab_shaped() {
        for app in ["Code", "Safari", "WeChat", "Finder"] {
            assert_no_crab_identity_leak(&get_offline_mutter(app, 14, "crab"));
        }
        assert_no_crab_identity_leak(&get_offline_mutter("Finder", 2, "crab"));
        assert_no_crab_identity_leak(startup_fallback_line("crab"));
        assert!(!startup_fallback_line("crab").contains("Nimbus"));
        assert_no_crab_identity_leak(style_switch_line("crab"));
        for line in touch_response_lines("crab") {
            assert_no_crab_identity_leak(line);
        }
        for weather in ["sunny", "windy", "rainy", "snowy"] {
            assert_no_crab_identity_leak(weather_feedback_line("crab", weather));
        }
    }

    #[test]
    fn crab_prompt_uses_positive_style_guidance() {
        let prompt = get_persona_prompt("default", "crab", "Nimbus", "sunny");

        assert!(prompt.contains("小螃蟹"));
        assert!(prompt.contains("大螯"));
        assert!(prompt.contains("过去造型"));
        assert!(!prompt.contains("Nimbus"));
        assert!(!prompt.contains("绝对不要自称"));
    }

    #[test]
    fn persona_prompt_suppresses_legacy_default_name() {
        let prompt = get_persona_prompt("default", "nimbus", "Nimbus", "sunny");

        assert!(!prompt.to_lowercase().contains("nimbus"));
        assert!(!prompt.contains("名字叫"));
    }

    #[test]
    fn persona_prompt_keeps_user_custom_name() {
        let prompt = get_persona_prompt("default", "crab", "小贝", "sunny");

        assert!(prompt.contains("名字叫小贝"));
        assert!(!prompt.contains("Nimbus"));
    }

    #[test]
    fn startup_line_filters_name_and_wrong_identity() {
        let raw = "（醒来时的动作）\n我——Nimbus！\n小云朵醒来啦\n小螃蟹举起大螯";

        assert_eq!(clean_startup_line(raw, "crab"), "小螃蟹举起大螯");
    }

    #[test]
    fn startup_line_falls_back_to_style_when_model_only_self_intros() {
        let raw = "我叫 Nimbus\n我是桌宠 Nimbus";

        assert_eq!(
            clean_startup_line(raw, "crab"),
            startup_fallback_line("crab")
        );
    }

    #[test]
    fn startup_line_filters_self_intro_and_case_variants() {
        let raw = "我是 nimbus，刚刚醒来\n我叫灵霓\n小螃蟹横着挪了一步";

        assert_eq!(clean_startup_line(raw, "crab"), "小螃蟹横着挪了一步");
    }

    #[test]
    fn startup_prompt_avoids_fixed_name_intro() {
        let prompt =
            build_startup_prompt("你记得自己最近说过：Nimbus 今天也上线了。", "crab", "sunny");

        assert!(!prompt.contains("Nimbus"));
        assert!(prompt.contains("不像自我介绍"));
        assert!(prompt.contains("小螃蟹"));
    }

    #[tokio::test]
    async fn test_model_mutter_generation_robustness() {
        let model_path = std::path::PathBuf::from("/Users/jinliang/.lmstudio/models/openbmb/MiniCPM5-1B-GGUF/MiniCPM5-1B-Q4_K_M.gguf");
        if !model_path.exists() {
            println!("Skipping Llama model test: model file not found.");
            return;
        }

        let engine = LlamaEngine::new(model_path, 30).expect("Failed to init LlamaEngine");
        
        let styles = ["crab", "mochi", "starlit", "nimbus"];
        let weather_modes = ["sunny", "windy", "rainy", "snowy"];
        let personas = ["tsundere", "toxic", "moe", "default"];
        
        use tokio_stream::StreamExt;

        for (i, style) in styles.iter().enumerate() {
            let weather = weather_modes[i % weather_modes.len()];
            let persona = personas[i % personas.len()];
            
            let short_persona = match persona {
                "tsundere" => "傲娇别扭，嘴硬心软",
                "toxic" => "毒舌犀利，幽默吐槽",
                "moe" => "极其软萌，超甜粘人",
                _ => "活泼可爱，贴贴治愈",
            };
            let short_style = match *style {
                "mochi" => "抹茶小猫（喜欢喵喵叫，说话带‘喵~’）",
                "starlit" => "太空章鱼（喜欢挥舞触手、咕噜噜吐泡泡）",
                "crab" => "红色小螃蟹（喜欢横着走、剪螯发出咔嚓声）",
                "nimbus" => "云朵精灵（喜欢在空中飘浮、说话轻飘飘）",
                _ => "可爱的桌宠",
            };
            let short_weather = match weather {
                "sunny" => "晴天☀️",
                "windy" => "刮大风💨",
                "rainy" => "下雨🌧️",
                "snowy" => "下雪❄️",
                _ => "晴天",
            };
            let prompt = format!(
                "你是桌宠Nimbus，性格是[{}]，外观是[{}]。主人当前在前台用[Code]，时间是14点，你的天气是{}。请配合设定自发无聊说一句话，绝对不要有任何格式，不能有英文，不超过15个字！",
                short_persona, short_style, short_weather
            );
            
            println!("=== Testing style: {}, weather: {}, persona: {} ===", style, weather, persona);
            
            let mut stream = engine.generate_mutter_stream(&prompt).await.expect("Failed to generate mutter");
            let mut response = String::new();
            while let Some(chunk) = tokio_stream::StreamExt::next(&mut stream).await {
                response.push_str(&chunk);
            }
            
            println!("Raw generated mutter: {:?}", response);
            let fallback = get_offline_mutter("Code", 14, style);
            let cleaned = clean_mutter_line(&response, style, &fallback);
            println!("Cleaned mutter: {:?}", cleaned);
            
            assert!(!cleaned.is_empty(), "Mutter should not be empty");
            
            let repeats = ["Hi", "Hello", "Crunch", "hi", "hello", "crunch", "嗝", "嘟", "哈"];
            for r in repeats {
                let count = cleaned.matches(r).count();
                assert!(count < 3, "Cleaned mutter contains too many repeated words {:?}: {:?}", r, cleaned);
            }
            
            assert!(!cleaned.contains('/') && !cleaned.contains('\\'), "Cleaned mutter contains weird formatting slashes: {:?}", cleaned);
            assert_no_crab_identity_leak_for_style(&cleaned, style);
        }
    }

    fn assert_no_crab_identity_leak_for_style(text: &str, style: &str) {
        let t = text.to_lowercase();
        match style {
            "crab" => {
                assert!(!t.contains("云") && !t.contains("猫") && !t.contains("喵") && !t.contains("章鱼"), "crab leaked: {text}");
            }
            "mochi" => {
                assert!(!t.contains("螃蟹") && !t.contains("螯") && !t.contains("云") && !t.contains("章鱼"), "mochi leaked: {text}");
            }
            "starlit" => {
                assert!(!t.contains("螃蟹") && !t.contains("螯") && !t.contains("猫") && !t.contains("喵") && !t.contains("云"), "starlit leaked: {text}");
            }
            _ => {
                assert!(!t.contains("螃蟹") && !t.contains("螯") && !t.contains("猫") && !t.contains("喵") && !t.contains("章鱼"), "nimbus leaked: {text}");
            }
        }
    }
}

