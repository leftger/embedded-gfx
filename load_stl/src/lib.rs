extern crate proc_macro;

use std::ops::Index;

use proc_macro::TokenStream;

#[proc_macro]
pub fn embed_stl(input: TokenStream) -> TokenStream {
    let file_path = input.to_string();

    let r = load_stl(file_path.trim_matches('"'));

    r.parse().unwrap()
}

fn write_vertices(vertices: &[stl_io::Vertex]) -> String {
    let mut out = String::new();
    for v in vertices {
        write!(&mut out, "[{}f32,{}f32,{}f32],", v[0], v[1], v[2]).unwrap();
    }
    out
}

fn write_faces(faces: &[stl_io::Triangle]) -> String {
    let mut out = String::new();
    for t in faces {
        write!(&mut out, "[{},{},{}],", t.vertices[0], t.vertices[1], t.vertices[2]).unwrap();
    }
    out
}

fn write_normals(faces: &[stl_io::Triangle]) -> String {
    let mut out = String::new();
    for t in faces {
        let n = t.normal;
        write!(&mut out, "[{}f32,{}f32,{}f32],", n[0], n[1], n[2]).unwrap();
    }
    out
}

fn write_lines(faces: &[stl_io::Triangle]) -> String {
    let lines = embedded_gfx::mesh::Geometry::lines_from_faces(
        &faces.iter().map(|f| [f.vertices[0], f.vertices[1], f.vertices[2]]).collect::<Vec<_>>(),
    );
    let mut out = String::new();
    for l in lines {
        write!(&mut out, "[{},{}],", l.0, l.1).unwrap();
    }
    out
}

fn load_stl(file_name: &str) -> String {
    use std::fmt::Write;

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .open(file_name)
        .unwrap();
    let stl = stl_io::read_stl(&mut file).unwrap();

    let vertices = write_vertices(&stl.vertices);
    let faces = write_faces(&stl.faces);
    let normals = write_normals(&stl.faces);
    let lines = write_lines(&stl.faces);

    let mut out = String::new();
    write!(
        &mut out,
        "Geometry {{
            vertices: &[
                {vertices}
            ],
            faces: &[
                {faces}
            ],
            colors: &[],
            lines: &[
                {lines}
            ],
            normals: &[
                {normals}
            ],
        }}"
    )
    .unwrap();

    out
}
