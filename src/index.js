const { execFile } = require("child_process");
const path = require("path");
const fs = require("fs-extra");

function execFileAsync(command, args) {
  return new Promise((resolve, reject) => {
    execFile(command, args, (err, stdout, stderr) => {
      if (err) {
        console.log(`Error calling exec: ${command} ${args.join(' ')}`);
        console.log("STDOUT:", stdout);
        console.error("STDERR:", stderr);
        reject(err);
      }

      resolve({ stdout, stderr });
    });
  });
}

module.exports = async function generateUnlitTextures(gltfPath, outPath) {
  let args = [gltfPath];

  if (outPath) {
    args = args.concat(["-o", outPath]);
  }

  const gltf = await fs.readJSON(gltfPath);

  if (!gltf.materials) {
    return;
  }

  const { stdout } = await execFileAsync("gltf_unlit_generator", args);

  const generatedTextures = JSON.parse(stdout.trim());

  const originalMaterialCount = gltf.materials.length;
  let nextUnlitMaterial = originalMaterialCount;

  // If we generated at least one unlit texture add the extensions used.
  if (generatedTextures.some((t) => t !== null)) {
    if (!gltf.extensionsUsed) {
      gltf.extensionsUsed = [];
    }

    gltf.extensionsUsed.push("MOZ_alt_materials", "KHR_materials_unlit");
  }

  for (let i = 0; i < originalMaterialCount; i++) {
    // Skip any materials that did not generate unlit textures
    if (generatedTextures[i] === null) {
      continue;
    }

    // Add the KHR_materials_unlit extension to the original material
    const originalMaterial = gltf.materials[i];

    if (!originalMaterial.extensions) {
      originalMaterial.extensions = {};
    }

    originalMaterial.extensions.MOZ_alt_materials = {
      KHR_materials_unlit: nextUnlitMaterial++
    };

    // Add the new unlit image to gltf.images
    const generatedTexture = generatedTextures[i];

    gltf.images.push({
      uri: path.basename(generatedTexture)
    });


    // Add the new unlit texture to gltf.textures
    gltf.textures.push({
      source: gltf.images.length - 1
    });

    // Add the new unlit material to gltf.materials
    gltf.materials.push({
      pbrMetallicRoughness: {
        baseColorTexture: {
          index: gltf.textures.length - 1
        },
        // Set fallback values for metallic and roughness factors
        roughnessFactor: 0.9,
        metallicFactor: 0.0
      },
      extensions: {
        KHR_materials_unlit: {}
      }
    });
  }

  // Overwrite the gltf file with the modified gltf file.
  const gltfFileName = path.basename(gltfPath);
  const outGltfPath = path.join(outPath, gltfFileName);
  await fs.writeJson(outGltfPath, gltf);
};
