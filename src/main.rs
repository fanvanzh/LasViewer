#[macro_use]
extern crate gfx;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate las;
extern crate cgmath;
extern crate clap;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate env_logger;

use gfx::traits::FactoryExt;
use gfx::Device;
use gfx_window_glutin as gfx_glutin;
use gfx::state::*;
use glutin::{GlRequest};
use glutin::Api::OpenGl;
use glutin::{Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use las::Reader;
//use cgmath::prelude::*;
use cgmath::{Point3, Vector3, Matrix4};
use clap::{Arg, App};
use chrono::prelude::*;
use log::{Level, LevelFilter};

pub type ColorFormat = gfx::format::Srgba8;
pub type DepthFormat = gfx::format::DepthStencil;

const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
gfx_defines!{
    vertex Vertex {
        pos: [f32; 3] = "a_Pos",
        color: [f32; 3] = "a_Color",
    }

    constant Transform {
        transform: [[f32; 4];4] = "u_Transform",
    }

    pipeline pipe {
        vbuf: gfx::VertexBuffer<Vertex> = (),
        transform: gfx::ConstantBuffer<Transform> = "Transform",
        out: gfx::RenderTarget<ColorFormat> = "Target0",
    }
}

const VS: &str = r#"
#version 150 core

in vec3 a_Pos;
in vec3 a_Color;

uniform Transform {
    mat4 u_Transform;
};

out vec4 v_Color;

void main() {
    v_Color = vec4(a_Color, 1.0);
    gl_Position = vec4(a_Pos, 1.0) * u_Transform;
}
"#;

const FS: &str = r#"
#version 150 core

in vec4 v_Color;
out vec4 Target0;

void main() {
    Target0 = v_Color;
}
"#;

#[derive(Debug)]
struct Bbox {
    minx: f32,
    miny: f32,
    minz: f32,
    maxx: f32,
    maxy: f32,
    maxz: f32,
}

impl Bbox {
    // add code here
    pub fn new(x: f32, y: f32, z: f32) -> Bbox {
        return Bbox {
            minx: x, maxx: x,
            miny: y, maxy: y,
            minz: z, maxz: z,
        }
    }

    pub fn extend(&mut self, x: f32, y: f32, z: f32) {
        if x < self.minx { self.minx = x; } else { self.maxx = x; }
        if y < self.miny { self.miny = y; } else { self.maxy = y; }
        if z < self.minz { self.minz = z; } else { self.maxz = z; }
    }

    pub fn center(&self) -> (f32,f32,f32) {
        return (
            (self.maxx + self.minx) / 2.0,
            (self.maxy + self.miny) / 2.0,
            (self.maxz + self.minz) / 2.0,
        );
    }
}

pub fn make_matrix(center: (f32,f32,f32)) -> Matrix4<f32> {
    let eye = Point3::new(center.0, center.1, center.2 + 50.0);
    let target = Point3::new(center.0, center.1, center.2);
    let view = Matrix4::look_at(eye, target, Vector3::unit_y());
    // A perspective projection.
    let projection = cgmath::perspective(
        cgmath::Rad(3.14 / 2.0), 16.0 / 9.0,1.0, 1000.0);
    //let model = Matrix4::<f32>::diagonal();
    let model_view = view;
    return projection * model_view;
}

fn apply_matrix(matrix: &Matrix4<f32>, transf: &mut Transform) {
//    for i in 0..16 {
//        let c = i / 4;
//        let l = i % 4;
//        transf.transform[l][c] = matrix.as_slice()[i];
//    }
    for y in 0..4 {
        transf.transform[0][y] = matrix.x[y];
        transf.transform[1][y] = matrix.y[y];
        transf.transform[2][y] = matrix.z[y];
        transf.transform[3][y] = matrix.w[y];
    }
}

fn init_log() -> env_logger::Builder{
    use std::env;
    use std::io::Write;
    if let Err(_) = env::var("RUST_LOG") {
        env::set_var("RUST_LOG", "info");
    }
    env::set_var("RUST_BACKTRACE", "1");
    let mut builder = env_logger::Builder::from_default_env();
    builder
        .format(|buf, record| {
            let dt = Local::now();
            let mut style = buf.style();
            if record.level() <= Level::Error {
                style.set_color(env_logger::Color::Red);
            } else {
                style.set_color(env_logger::Color::Green);
            }
            writeln!(
                buf,
                "{}: {} - {}",
                style.value(record.level()),
                dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .init();
    builder
}

fn init_matches() -> clap::ArgMatches<'static>{
    App::new("LasViewer")
        .version("1.0")
        .author("fanvanzh")
        .about("very simple tool to view las data")
        .arg(
            Arg::with_name("input")
                .short("i")
                .value_name("FILE")
                .help("the input file")
                .required(true)
                .takes_value(true)
        )
        .get_matches()
}

pub fn main() {
    init_log();
    let matches = init_matches();
    let input = matches.value_of("input").unwrap();
    let in_path = std::path::Path::new(input);
    if !in_path.exists() {
        error!("{} does not exists.", input);
        return;
    }
    let mut point_cloud: Vec<Vertex> = Vec::new();
    let mut reader = Reader::from_path(input).unwrap();
    for wrapped_point in reader.points()
    {
        let point = wrapped_point.unwrap();
        if let Some(color) = point.color {
            point_cloud.push( Vertex{
                pos: [point.x as f32, point.y as f32, point.z as f32],
                color: [color.red as f32 / 65535.0, color.green as f32 / 65535.0, color.blue as f32 / 65535.0]
            });
        }
    }
    if point_cloud.len() == 0 {
        error!("no data read");
        return;
    }
    //TODO:: add a Loading Animation
    let mut events_loop = glutin::EventsLoop::new();
    let windowbuilder = glutin::WindowBuilder::new()
        .with_title("LasViewer".to_string())
        .with_dimensions((800, 600).into());
    let contextbuilder = glutin::ContextBuilder::new()
        .with_gl(GlRequest::Specific(OpenGl,(3,2)))
        .with_vsync(true);
    let (window, mut device, mut factory, color_view, mut depth_view) =
        gfx_glutin::init::<ColorFormat, DepthFormat>(windowbuilder, contextbuilder, &events_loop).unwrap();

    let program = factory.link_program(
        VS.as_bytes(),
        FS.as_bytes() ).unwrap();
    let raster = Rasterizer {
        front_face: FrontFace::Clockwise,
        cull_face: CullFace::Nothing,
        method: RasterMethod::Point,
        offset: None, 
        samples: None
    };
    let pso = factory.create_pipeline_from_program(
        &program,
        gfx::Primitive::PointList,
        raster,
        pipe::new()
        ).unwrap();
    let mut encoder: gfx::Encoder<_, _> = factory.create_command_buffer().into();

    let mut bbox = Bbox::new(point_cloud[0].pos[0],point_cloud[0].pos[1],point_cloud[0].pos[2]);
    for pt in point_cloud.iter() {
        bbox.extend(pt.pos[0],pt.pos[1],pt.pos[2])
    }
    let matrix = make_matrix( bbox.center());
    //Identity Matrix
    let mut transf = Transform { transform: [[0.0;4],[0.0;4],[0.0;4],[0.0;4]] };
    apply_matrix(&matrix, &mut transf);

    let (vertex_buffer, slice) = factory.create_vertex_buffer_with_slice(point_cloud.as_slice(), ());
    let transform_buffer = factory.create_constant_buffer(1);
    let mut data = pipe::Data {
        vbuf: vertex_buffer,
        transform: transform_buffer,
        out: color_view.clone(),
    };
    let mut running = true;
    while running { 
        events_loop.poll_events(|event| {
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested |
                    WindowEvent::KeyboardInput {
                        input: KeyboardInput {
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        },
                        ..
                    } => running = false,
                    WindowEvent::Resized(size) => {
                        window.resize(size.to_physical(window.get_hidpi_factor()));
                        gfx_window_glutin::update_views(&window, &mut data.out, &mut depth_view);
                    },
                    _ => (),
                }
            }
        });

        // Put in main loop before swap buffers and device clean-up method
        encoder.clear(&color_view, BLACK); //clear the framebuffer with a color(color needs to be an array of 4 f32s, RGBa)
        encoder.update_buffer(&data.transform, &[transf], 0).unwrap(); //update buffers
        encoder.draw(&slice, &pso, &data); // draw commands with buffer data and attached pso
        encoder.flush(&mut device); // execute draw commands
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}
