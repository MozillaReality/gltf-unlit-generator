extern crate image;
extern crate clap;
extern crate gltf;
extern crate serde_json;

use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process;

use clap::{App, Arg};
use image::{ImageError, RgbaImage};
use gltf::{Gltf, Material, Texture};
use gltf::image::Data;
use serde_json::Value as JsonValue;

fn main() {
    let matches = App::new("gltf_unlit_generator")
        .version("0.1")
        .about("Generates an unlit texture for a .gltf file.")
        .args_from_usage("[input] 'input .gltf file'")
        .arg(Arg::with_name("out")
            .short("o")
            .long("out")
            .value_name("out")
            .help("Where to output the texture files.")
            .takes_value(true))
        .get_matches();

    if let Some(gltf_file_path) = matches.value_of("input") {
        if let Ok(file) = File::open(gltf_file_path) {
            let gltf_path = Path::new(gltf_file_path).parent().unwrap();

            let out_path = matches.value_of("out")
                .map_or(gltf_path.clone(), |out| Path::new(out));

            let gltf = Gltf::from_reader(BufReader::new(file)).unwrap().validate_minimally().unwrap();
            let generated_textures: Vec<_> = gltf.materials()
                .map(|material| generate_unlit(gltf_path, out_path, material))
                .collect();

            println!("{}", JsonValue::Array(generated_textures));
            process::exit(0);
        }

        println!("filePath: {}", gltf_file_path);
    }

    println!("{}", JsonValue::Null);
    process::exit(1);
}

fn apply_occlusion(img: &mut RgbaImage, occlusion_map: &RgbaImage, strength: f32) {
    let multiplier = strength / 255.0;
    for (mut pixel, occ) in img.pixels_mut().zip(occlusion_map.pixels()) {
        // Occlusion is on the red channel of the occlusion texture
        let occlusion_factor = occ[0] as f32 * multiplier;
        pixel.data[0] = (pixel.data[0] as f32 * occlusion_factor) as u8;
        pixel.data[1] = (pixel.data[1] as f32 * occlusion_factor) as u8;
        pixel.data[2] = (pixel.data[2] as f32 * occlusion_factor) as u8;
    }
}

fn apply_emissive(img: &mut RgbaImage, emissive_map: &RgbaImage, color: [f32; 3]) {
    for (mut pixel, em) in img.pixels_mut().zip(emissive_map.pixels()) {
        let emissive_r = ((em.data[0] as f32) * color[0]) as u8;
        let emissive_g = ((em.data[1] as f32) * color[1]) as u8;
        let emissive_b = ((em.data[2] as f32) * color[2]) as u8;

        pixel.data[0] = pixel.data[0].saturating_add(emissive_r);
        pixel.data[1] = pixel.data[1].saturating_add(emissive_g);
        pixel.data[2] = pixel.data[2].saturating_add(emissive_b);
    }
}

fn generate_unlit(gltf_dir: &Path, out_path: &Path, material: Material) -> JsonValue {
    let pbr = material.pbr_metallic_roughness();

    let base_color_factor = pbr.base_color_factor();

    // Set the unlit_map to the base color map if it exists
    let unlit_map = pbr.base_color_texture().and_then(|info| load_image(gltf_dir, &info.texture())).map(|mut base_color_map| {
        for mut pixel in base_color_map.pixels_mut() {
            pixel.data[0] = (pixel.data[0] as f32 * base_color_factor[0]) as u8;
            pixel.data[1] = (pixel.data[1] as f32 * base_color_factor[1]) as u8;
            pixel.data[2] = (pixel.data[2] as f32 * base_color_factor[2]) as u8;
            pixel.data[3] = (pixel.data[3] as f32 * base_color_factor[3]) as u8;
        }
        base_color_map
    });

    // Multiply the occlusion map if it exists
    let occlusion_texture = material.occlusion_texture();
    let occlusion_strength = occlusion_texture.as_ref().map_or(0.0, |t| t.strength());
    let occlusion_map = occlusion_texture.and_then(|info| load_image(gltf_dir, &info.texture()));
    let unlit_map = match occlusion_map {
        Some(occlusion_map) => {
            let mut unlit_map = unlit_map.unwrap_or_else(|| occlusion_map.clone());
            apply_occlusion(&mut unlit_map, &occlusion_map, occlusion_strength);
            Some(unlit_map)
        },
        None => unlit_map
    };

    // Add the emissive map if it exists
    let emissive_texture = material.emissive_texture();
    let emissive_factor = material.emissive_factor();
    let emissive_map = emissive_texture.and_then(|info| load_image(gltf_dir, &info.texture()));
    let unlit_map = match emissive_map {
        Some(emissive_map) => {
            let mut unlit_map = unlit_map.unwrap_or_else(|| emissive_map.clone());
            apply_emissive(&mut unlit_map, &emissive_map, emissive_factor);
            Some(unlit_map)
        },
        None => unlit_map
    };

    match unlit_map {
        Some(unlit_map) => {

            let path = match material.name() {
                Some(name) => format!("{}_unlit.png", name),
                None => format!("unlit_{}.png", material.index().unwrap())
            };

            fs::create_dir_all(&out_path).unwrap();

            let fout = &out_path.join(path);
            unlit_map.save(fout).unwrap();

            JsonValue::String(String::from(fout.to_str().unwrap()))
        }
        None => JsonValue::Null
    }
}

fn load_image(dir: &Path, texture: &Texture) -> Option<RgbaImage> {
    let load_result = match texture.source().data() {
        Data::Uri { uri, .. } => image::open(dir.join(uri)).map(|i| i.to_rgba()),
        Data::View { .. } => Err(ImageError::FormatError(String::from("Images in data views not supported.")))
    };
    match load_result {
        Ok(img) => Some(img),
        Err(e) => {
            eprintln!("{}", e);
            None
        }
    }
}
