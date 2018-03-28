extern crate image;
extern crate clap;
extern crate gltf;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process;

use clap::App;

use gltf::Gltf;
use gltf::Material;
use gltf::image::Data;

fn main() {
    let matches = App::new("gltf_unlit_generator")
        .version("0.1")
        .about("Generates an unlit texture for a .gltf file.")
        .args_from_usage("[input] 'input .gltf file'")
        .get_matches();

    if let Some(gltf_file_path) = matches.value_of("input") {
        if let Ok(file) = File::open(gltf_file_path) {
            let gltf = Gltf::from_reader(BufReader::new(file)).unwrap().validate_minimally().unwrap();

            let gltf_path = Path::new(gltf_file_path).parent().unwrap();

            for material in gltf.materials() {
                generate_unlit(gltf_path, material)
            }

            println!("Done!");
            process::exit(0);
        }
    }

    eprintln!("Please provide a path to a .gltf file");
    process::exit(1);
}

fn generate_unlit<'a>(gltf_path: &Path, material: Material) {
    let pbr = material.pbr_metallic_roughness();

    // Set the unlit_map to the base color map if it exists
    let unlit_map = load_texture_info_image(gltf_path, pbr.base_color_texture());

    let occlusion_texture = material.occlusion_texture();

    // Multiply the occlusion map if it exists
    let unlit_map = match occlusion_texture {
        Some(occlusion_texture) => {
            let occlusion_strength = occlusion_texture.strength();
            
            match load_texture_image(gltf_path, occlusion_texture.texture()) {
                Some(occlusion_map) => {
                    let mut unlit_map = unlit_map.map_or(occlusion_map.clone(), |unlit_map| unlit_map);

                    for (x, y, mut pixel) in unlit_map.enumerate_pixels_mut() {
                        // Occlusion is on the red channel of the occlusion texture
                        let occlusion_value = occlusion_map.get_pixel(x, y)[0];
                        let occlusion_factor = (occlusion_value as f32 / 255.0) * occlusion_strength;

                        pixel.data[0] = (pixel.data[0] as f32 * occlusion_factor) as u8;
                        pixel.data[1] = (pixel.data[1] as f32 * occlusion_factor) as u8;
                        pixel.data[2] = (pixel.data[2] as f32 * occlusion_factor) as u8;
                    }

                    Some(unlit_map)
                },
                None => None
            }
        },
        None => None
    };

    let emissive_factor = material.emissive_factor();
    let emissive_map = load_texture_info_image(gltf_path, material.emissive_texture());

    // Add the emissive map if it exists
    let unlit_map = match emissive_map {
        Some(emissive_map) => {
            let mut unlit_map = unlit_map.map_or(emissive_map.clone(), |unlit_map| unlit_map);

            for (x, y, mut pixel) in unlit_map.enumerate_pixels_mut() {
                let emissive = emissive_map.get_pixel(x, y);

                let emissive_r = ((emissive.data[0] as f32) * emissive_factor[0]) as u8;
                let emissive_g = ((emissive.data[1] as f32) * emissive_factor[1]) as u8;
                let emissive_b = ((emissive.data[2] as f32) * emissive_factor[2]) as u8;

                pixel.data[0] = pixel.data[0].saturating_add(emissive_r);
                pixel.data[1] = pixel.data[1].saturating_add(emissive_g);
                pixel.data[2] = pixel.data[2].saturating_add(emissive_b);
            }

            Some(unlit_map)
        },
        None => None
    };
    
    match unlit_map {
        Some(unlit_map) => {
            
            let path = match material.name() {
                Some(name) => format!("{}_unlit.png", name),
                None => format!("unlit_{}.png", material.index().unwrap())
            };
            let fout = &gltf_path.join(path);
            unlit_map.save(fout).unwrap();
        }
        None => ()
    }
}

fn load_texture_info_image(gltf_path: &Path, texture_info: Option<gltf::texture::Info>) -> Option<image::RgbaImage> {
    match texture_info {
        Some(texture_info) => {
            load_texture_image(gltf_path, texture_info.texture())
        },
        None => None
    }
}

fn load_texture_image(gltf_path: &Path, texture: gltf::Texture) -> Option<image::RgbaImage> {
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
            println!("Warning: Images in data views not currently supported. Skipping image[{}].", image.index());
            None
        }
    }
}