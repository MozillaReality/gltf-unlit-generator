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

use image::RgbaImage;

use gltf::Gltf;
use gltf::Material;
use gltf::image::Data;
use gltf::Texture;
use gltf::texture::Info as TextureInfo;

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

fn generate_unlit<'a>(gltf_path: &Path, out_path: &Path, material: Material) -> JsonValue {
    let pbr = material.pbr_metallic_roughness();

    let base_color_factor = pbr.base_color_factor();

    // Set the unlit_map to the base color map if it exists
    let unlit_map = load_texture_info_image(gltf_path, pbr.base_color_texture()).map(|mut base_color_texture| {
        for mut pixel in base_color_texture.pixels_mut() {
            pixel.data[0] = (pixel.data[0] as f32 * base_color_factor[0]) as u8;
            pixel.data[1] = (pixel.data[1] as f32 * base_color_factor[1]) as u8;
            pixel.data[2] = (pixel.data[2] as f32 * base_color_factor[2]) as u8;
            pixel.data[3] = (pixel.data[3] as f32 * base_color_factor[3]) as u8;
        }
        base_color_texture
    });

    let occlusion_texture = material.occlusion_texture();

    // Multiply the occlusion map if it exists
    let unlit_map = match occlusion_texture {
        Some(occlusion_texture) => {
            let occlusion_multiplier = occlusion_texture.strength() / 255.0;

            match load_texture_image(gltf_path, occlusion_texture.texture()) {
                Some(occlusion_map) => {
                    let mut unlit_map = unlit_map.unwrap_or_else(|| occlusion_map.clone());

                    for (mut pixel, occlusion) in unlit_map.pixels_mut().zip(occlusion_map.pixels()) {
                        // Occlusion is on the red channel of the occlusion texture
                        let occlusion_factor = occlusion[0] as f32 * occlusion_multiplier;
                        pixel.data[0] = (pixel.data[0] as f32 * occlusion_factor) as u8;
                        pixel.data[1] = (pixel.data[1] as f32 * occlusion_factor) as u8;
                        pixel.data[2] = (pixel.data[2] as f32 * occlusion_factor) as u8;
                    }

                    Some(unlit_map)
                },
                None => unlit_map
            }
        },
        None => unlit_map
    };

    let emissive_factor = material.emissive_factor();
    let emissive_map = load_texture_info_image(gltf_path, material.emissive_texture());

    // Add the emissive map if it exists
    let unlit_map = match emissive_map {
        Some(emissive_map) => {
            let mut unlit_map = unlit_map.unwrap_or_else(|| emissive_map.clone());

            for (mut pixel, emissive) in unlit_map.pixels_mut().zip(emissive_map.pixels()) {
                let emissive_r = ((emissive.data[0] as f32) * emissive_factor[0]) as u8;
                let emissive_g = ((emissive.data[1] as f32) * emissive_factor[1]) as u8;
                let emissive_b = ((emissive.data[2] as f32) * emissive_factor[2]) as u8;

                pixel.data[0] = pixel.data[0].saturating_add(emissive_r);
                pixel.data[1] = pixel.data[1].saturating_add(emissive_g);
                pixel.data[2] = pixel.data[2].saturating_add(emissive_b);
            }

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

fn load_texture_info_image(gltf_path: &Path, texture_info: Option<TextureInfo>) -> Option<RgbaImage> {
    match texture_info {
        Some(texture_info) => {
            load_texture_image(gltf_path, texture_info.texture())
        },
        None => None
    }
}

fn load_texture_image(gltf_path: &Path, texture: Texture) -> Option<RgbaImage> {
    let image = texture.source();

    match image.data() {
        Data::Uri { uri, .. } => {
            if let Ok(image) = image::open(gltf_path.join(uri)) {
                Some(image.to_rgba())
            } else {
                panic!("Invalid image path {}", uri);
            }
        },
        Data::View { .. } => {
            eprintln!("Warning: Images in data views not currently supported. Skipping image[{}].", image.index());
            None
        }
    }
}
