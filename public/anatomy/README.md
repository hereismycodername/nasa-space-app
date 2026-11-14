# BodyParts3D reference anatomy

`body.glb` contains real three-dimensional organ, bone and skin surfaces from
BodyParts3D 4.0, using the same anatomical concepts as the existing frontal map.
It is a reference adult male, not an individual astronaut's scan. Hands and feet
are represented by the skin surface only. Missing anatomical structures are not
invented or reconstructed from the older SVG projection.

## Attribution and licence

BodyParts3D, © The Database Center for Life Science licensed under CC Attribution
4.0 International.

- [Official licence and required attribution](https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html)
- [CC BY 4.0 licence](https://creativecommons.org/licenses/by/4.0/)
- [Official download page](https://dbarchive.biosciencedbc.jp/en/bodyparts3d/download.html)

The archived OBJ source headers also carry the older Creative Commons
Attribution-Share Alike 2.1 Japan notice. Those notices are retained verbatim in
`body.provenance.json`. This adapted geometry is additionally shared under those
[CC BY-SA 2.1 Japan terms](https://creativecommons.org/licenses/by-sa/2.1/jp/).
This statement applies to the derived geometry, not the application's code.

## Regeneration

Download these two data files from the official archive into a temporary directory:

- [partof_BP3D_4.0_obj_99.zip](https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/partof_BP3D_4.0_obj_99.zip)
- [partof_element_parts.txt](https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/partof_element_parts.txt)

Run from the repository root with Python 3.11+ and the project's Node version.
Python needs only the standard library. Meshoptimizer is a pinned build-time tool
installed in a temporary directory; it is not added to the application runtime:

```sh
python3 scripts/generate-anatomy-model.py \
  /path/to/partof_BP3D_4.0_obj_99.zip \
  /path/to/partof_element_parts.txt \
  --output /path/to/cache/body.raw.glb
npm install --prefix /path/to/cache/tools --ignore-scripts --no-audit --no-fund meshoptimizer@1.3.0
node scripts/simplify-anatomy-model.mjs \
  /path/to/cache/body.raw.glb public/anatomy/body.glb \
  /path/to/cache/tools/node_modules/meshoptimizer/meshopt_simplifier.js
```

The generator reads OBJ records directly from the ZIP without extracting paths
or executing source content. The generated provenance file records the exact
input SHA-256 hashes, anatomical concept IDs, output hash and geometry counts.
The upstream `LATEST` URLs may change; match the recorded hashes to reproduce
this exact asset. Only the small generated GLB and provenance are shipped; the
source archive and intermediate GLB are not needed at application runtime.
The delivered model is 1,597,376 bytes with 43,669 vertices and 85,276 triangles
across 40 independently selectable/materialled meshes. The pipeline rejects
results above 3 MB or 100,000 triangles.

## Transformations and renderer contract

- Original three-dimensional surfaces are simplified with meshoptimizer 1.3.0
  (quadric-error reduction with a 1% relative error bound, exact vertex welding
  and removal of tiny isolated components). Targets range from 500 triangles
  per eye to 15,000 for the skin. Per-part counts and achieved errors are recorded
  in the provenance file. Small details are reduced for browser performance;
  this is not a model for anatomical measurement.
- No extrusion of SVG outlines, textures, remote glTF resources or runtime
  compression decoders are used.
- Meshes are grouped by anatomical concept and assigned the existing map's
  tissue colours. Colour values are converted from sRGB to glTF linear RGB.
- The skin bounding-box centre is moved to the origin and its height is scaled
  to two units. Y points upwards, Z points towards the front of the body, and
  the patient's left is positive X. Laterality is verified using the lungs and
  the anterior direction using the sternum relative to the vertebral column.
- Winding is corrected after the axis transformation. Smooth normals are
  recomputed after simplification by accumulating triangle normals weighted by
  area. The delivered skin height is approximately two units after reduction.
- Every mesh node has `extras.system` (`brain`, `eyes`, `heart`, `lungs`, `gut`,
  `msk` or `immune`) and `extras.layer` (`skin`, `skeleton` or `organ`). Three.js
  exposes these fields in `mesh.userData`. `extras.concept` is the FMA ID.
- Materials start opaque; the application controls visibility and transparency
  for the organs, skeleton and X-ray modes. Colours convey anatomy and selection;
  they do not diagnose health.

The GLB is served locally with the application so rotation does not require an
internet connection after the application and its assets have loaded.
