#[macro_use]
extern crate gfx;
extern crate gfx_window_glutin;
extern crate glutin;
extern crate las;
extern crate nalgebra as na;

use gfx::traits::FactoryExt;
use gfx::Device;
use gfx_window_glutin as gfx_glutin;
use gfx::state::*;
use glutin::{GlContext, GlRequest};
use glutin::Api::OpenGl;
use las::Reader;
use na::{Isometry3, Perspective3, Point3, Vector3, Matrix4};

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

const vs: &str = r#"
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

const fs: &str = r#"
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
}

pub fn make_matrix(x: f32, y: f32, z: f32) -> Matrix4<f32> {
    let model = Isometry3::new(Vector3::x(), na::zero());
    // Our camera looks toward the point (1.0, 0.0, 0.0).
    // It is located at (0.0, 0.0, 1.0).
    let eye = Point3::new(x, y, z - 20.0);
    let target = Point3::new(x, y, z);
    let view = Isometry3::look_at_rh(&eye, &target, &Vector3::y());

    // A perspective projection.
    let projection = Perspective3::new(16.0 / 9.0, 3.14 / 2.0, 1.0, 1000.0);

    // The combination of the model with the view is still an isometry.
    let model_view = view * model;

    // Convert everything to a `Matrix4` so that they can be combined.
    let mat_model_view = model_view.to_homogeneous();

    // Combine everything.
    return projection.as_matrix() * mat_model_view;

    //println!("matrix is {:?}", model_view_projection.as_slice());
}

fn apply_matrix(matrix: &Matrix4<f32>, transf: &mut Transform) {
    for i in 0..16 {
        let c = i / 4;
        let l = i % 4;
        transf.transform[l][c] = matrix.as_slice()[i];
    }
}

pub fn main() {
    let mut events_loop = glutin::EventsLoop::new();
    let windowbuilder = glutin::WindowBuilder::new()
        .with_title("Point Cloud".to_string())
        .with_dimensions(800, 600);
    let contextbuilder = glutin::ContextBuilder::new()
        .with_gl(GlRequest::Specific(OpenGl,(3,2)))
        .with_vsync(true);
    let (window, mut device, mut factory, mut color_view, mut depth_view) =
        gfx_glutin::init::<ColorFormat, DepthFormat>(windowbuilder, contextbuilder, &events_loop);

    let program = factory.link_program(
        vs.as_bytes(),
        fs.as_bytes() ).unwrap();
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
    let mut point_cloud: Vec<Vertex> = Vec::new();
    let mut reader = Reader::from_path(r#"E:\Data\Tile_1\Tile_1.las"#).unwrap();    
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
    let mut bbox = Bbox::new(point_cloud[0].pos[0],point_cloud[0].pos[1],point_cloud[0].pos[2]);
    for pt in point_cloud.iter() {
        bbox.extend(pt.pos[0],pt.pos[1],pt.pos[2])
    }
    let matrix = make_matrix( 
        (bbox.minx + bbox.maxx) / 2.0, 
        (bbox.miny + bbox.maxy) / 2.0, 
        (bbox.minz + bbox.maxz) / 2.0
        );
    //Identity Matrix
    let mut transf = Transform { transform: [[0.0;4],[0.0;4],[0.0;4],[0.0;4]] };
    apply_matrix(&matrix, &mut transf);

    let (vertex_buffer, slice) = factory.create_vertex_buffer_with_slice(point_cloud.as_slice(), ());
    let transform_buffer = factory.create_constant_buffer(1);
    let data = pipe::Data {
        vbuf: vertex_buffer,
        transform: transform_buffer,
        out: color_view.clone(),
    };
    let mut running = true;
    while running { 
        events_loop.poll_events(|event| {
            if let glutin::Event::WindowEvent { event, .. } = event {
                match event {
                    glutin::WindowEvent::Closed |
                    glutin::WindowEvent::KeyboardInput {
                        input: glutin::KeyboardInput {
                            virtual_keycode: Some(glutin::VirtualKeyCode::Escape), ..
                        }, ..
                    } => running = false,
                    glutin::WindowEvent::Resized(_, _) => {
                        gfx_glutin::update_views(&window, &mut color_view, &mut depth_view);
                    },
                    _ => {}
                }
            }
        });

        // Put in main loop before swap buffers and device clean-up method
        encoder.clear(&color_view, BLACK); //clear the framebuffer with a color(color needs to be an array of 4 f32s, RGBa)
        encoder.update_buffer(&data.transform, &[transf], 0); //update buffers
        encoder.draw(&slice, &pso, &data); // draw commands with buffer data and attached pso
        encoder.flush(&mut device); // execute draw commands
        window.swap_buffers().unwrap();
        device.cleanup();
    }
}
