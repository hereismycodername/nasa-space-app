#!/usr/bin/env python3
"""Convert official BodyParts3D 4.0 OBJ meshes to an intermediate glTF 2 GLB.

Python standard library only. Run simplify-anatomy-model.mjs before shipping it.
No downloads, executable assets or runtime CDN.
See public/anatomy/README.md for source URLs, licences and the regeneration command.
"""

import argparse
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import struct
import zipfile


SOURCE_URL = 'https://dbarchive.biosciencedbc.jp/data/bodyparts3d/LATEST/'
COPYRIGHT = ('BodyParts3D, © The Database Center for Life Science licensed under '
             'CC Attribution 4.0 International')
BONE = '#e4dac4'
# Same anatomical concepts as ui/tools/gen_anatomy.py; original 3D surfaces are
# preserved instead of reconstructing depth from the older frontal SVG outlines.
PARTS = [
    ('immune', 'Skin', 'skin', '#52657b', 'FMA7163'),
    ('msk', 'Skull', 'skeleton', BONE, 'FMA46565'),
    ('msk', 'Vertebral column', 'skeleton', BONE, 'FMA13478'),
    ('msk', 'Rib cage', 'skeleton', BONE, 'FMA7480'),
    ('msk', 'Sternum', 'skeleton', BONE, 'FMA7485'),
    ('msk', 'Pelvis', 'skeleton', BONE, 'FMA16580'),
    *[('msk', name, 'skeleton', BONE, concept) for name, concept in [
        ('Right clavicle', 'FMA13322'), ('Left clavicle', 'FMA13323'),
        ('Right scapula', 'FMA13395'), ('Left scapula', 'FMA13396'),
        ('Right humerus', 'FMA23130'), ('Left humerus', 'FMA23131'),
        ('Right radius', 'FMA23464'), ('Left radius', 'FMA23465'),
        ('Right ulna', 'FMA23467'), ('Left ulna', 'FMA23468'),
        ('Right femur', 'FMA24474'), ('Left femur', 'FMA24475'),
        ('Right tibia', 'FMA24477'), ('Left tibia', 'FMA24478'),
        ('Right fibula', 'FMA24480'), ('Left fibula', 'FMA24481'),
    ]],
    ('gut', 'Large intestine', 'organ', '#c88a78', 'FMA7201'),
    ('gut', 'Small intestine', 'organ', '#d99f8c', 'FMA7200'),
    ('gut', 'Right kidney', 'organ', '#8e3b3f', 'FMA7204'),
    ('gut', 'Left kidney', 'organ', '#8e3b3f', 'FMA7205'),
    ('gut', 'Urinary bladder', 'organ', '#c98e8e', 'FMA15900'),
    ('gut', 'Pancreas', 'organ', '#d8a66e', 'FMA7198'),
    ('gut', 'Liver', 'organ', '#7a2e2a', 'FMA7197'),
    ('gut', 'Gallbladder', 'organ', '#4f7a3a', 'FMA7202'),
    ('gut', 'Stomach', 'organ', '#cf8b7b', 'FMA7148'),
    ('gut', 'Esophagus', 'organ', '#c48078', 'FMA7131'),
    ('lungs', 'Trachea', 'organ', '#a8b4c0', 'FMA7394'),
    ('lungs', 'Right lung', 'organ', '#c48a92', 'FMA7309'),
    ('lungs', 'Left lung', 'organ', '#c48a92', 'FMA7310'),
    ('immune', 'Thymus', 'organ', '#d9c28e', 'FMA9607'),
    ('heart', 'Heart', 'organ', '#b02e3a', 'FMA7088'),
    ('brain', 'Brain', 'organ', '#d7a3a0', 'FMA50801'),
    ('eyes', 'Right eye', 'organ', '#e8eaee', 'FMA12514'),
    ('eyes', 'Left eye', 'organ', '#e8eaee', 'FMA12515'),
]


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def read_obj(contents):
    vertices, triangles = [], []
    for line in contents.splitlines():
        fields = line.split()
        if fields[:1] == ['v']:
            point = tuple(float(value) for value in fields[1:4])
            if len(point) != 3 or not all(math.isfinite(value) for value in point):
                raise ValueError('Invalid OBJ position')
            vertices.append(point)
        elif fields[:1] == ['f']:
            face = [int(value.split('/')[0]) for value in fields[1:]]
            face = [index - 1 if index > 0 else len(vertices) + index for index in face]
            if any(index < 0 or index >= len(vertices) for index in face):
                raise ValueError('OBJ face index out of bounds')
            for index in range(1, len(face) - 1):
                triangles.append((face[0], face[index], face[index + 1]))
    if not vertices or not triangles:
        raise ValueError('Empty OBJ geometry')
    return vertices, triangles


def bounds(vertices):
    return ([min(point[axis] for point in vertices) for axis in range(3)],
            [max(point[axis] for point in vertices) for axis in range(3)])


def center(vertices):
    return [sum(point[axis] for point in vertices) / len(vertices) for axis in range(3)]


def normals_for(vertices, triangles):
    normals = [[0.0, 0.0, 0.0] for _ in vertices]
    for i, j, k in triangles:
        a = [vertices[j][axis] - vertices[i][axis] for axis in range(3)]
        b = [vertices[k][axis] - vertices[i][axis] for axis in range(3)]
        normal = (a[1] * b[2] - a[2] * b[1],
                  a[2] * b[0] - a[0] * b[2],
                  a[0] * b[1] - a[1] * b[0])
        for vertex in (i, j, k):
            for axis in range(3):
                normals[vertex][axis] += normal[axis]
    for index, normal in enumerate(normals):
        length = math.sqrt(sum(value * value for value in normal))
        normals[index] = [value / length for value in normal] if length else [0.0, 1.0, 0.0]
    return normals


def linear_color(hex_color):
    values = [int(hex_color[index:index + 2], 16) / 255 for index in (1, 3, 5)]
    return [value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4
            for value in values]


def generate(archive_path, elements_path, output):
    elements = defaultdict(list)
    for line in elements_path.read_text(encoding='utf-8-sig').splitlines()[1:]:
        columns = line.split('\t')
        if len(columns) >= 3:
            elements[columns[0]].append(columns[2])

    geometry, original_headers = {}, set()
    with zipfile.ZipFile(archive_path) as archive:
        members = {Path(name).stem: name for name in archive.namelist() if name.endswith('.obj')}
        for _, _, _, _, concept in PARTS:
            vertices, triangles = [], []
            if not elements[concept]:
                raise ValueError(f'No components for {concept}')
            for element in dict.fromkeys(elements[concept]):
                contents = archive.read(members[element]).decode('utf-8-sig')
                original_headers.update(line for line in contents.splitlines() if line.startswith('#'))
                positions, faces = read_obj(contents)
                offset = len(vertices)
                vertices.extend(positions)
                triangles.extend(tuple(index + offset for index in face) for face in faces)
            geometry[concept] = vertices, triangles

    # Source Z is superior. Derive laterality/anterior from known structures,
    # avoiding a visually plausible mirrored or back-facing model.
    left = center(geometry['FMA7310'][0])
    right = center(geometry['FMA7309'][0])
    x_sign = 1 if left[0] > right[0] else -1
    sternum = center(geometry['FMA7485'][0])
    spine = center(geometry['FMA13478'][0])
    anterior_sign = 1 if sternum[1] > spine[1] else -1
    skin_min, skin_max = bounds(geometry['FMA7163'][0])
    source_center = [(lower + upper) / 2 for lower, upper in zip(skin_min, skin_max)]
    scale = 2 / (skin_max[2] - skin_min[2])
    reverse_winding = -x_sign * anterior_sign < 0

    binary = bytearray()
    gltf = {
        'asset': {'version': '2.0', 'generator': 'scripts/generate-anatomy-model.py',
                  'copyright': COPYRIGHT},
        'scene': 0, 'scenes': [{'name': 'BodyParts3D reference anatomy', 'nodes': []}],
        'nodes': [], 'meshes': [], 'materials': [], 'accessors': [], 'bufferViews': [],
    }

    def accessor(rows, fmt, component_type, kind, target, include_bounds=False):
        while len(binary) % 4:
            binary.append(0)
        offset = len(binary)
        for row in rows:
            binary.extend(struct.pack('<' + fmt * len(row), *row))
        gltf['bufferViews'].append({'buffer': 0, 'byteOffset': offset,
                                    'byteLength': len(binary) - offset, 'target': target})
        result = {'bufferView': len(gltf['bufferViews']) - 1, 'componentType': component_type,
                  'count': len(rows), 'type': kind}
        if include_bounds:
            # Bounds refer to actual float32 values written into the file.
            quantized = [struct.unpack('<3f', struct.pack('<3f', *row)) for row in rows]
            result['min'], result['max'] = bounds(quantized)
        gltf['accessors'].append(result)
        return len(gltf['accessors']) - 1

    part_stats = []
    for system, name, layer, color, concept in PARTS:
        source_vertices, source_triangles = geometry[concept]
        vertices = [(x_sign * (x - source_center[0]) * scale,
                     (z - source_center[2]) * scale,
                     anterior_sign * (y - source_center[1]) * scale)
                    for x, y, z in source_vertices]
        triangles = [(a, c, b) if reverse_winding else (a, b, c) for a, b, c in source_triangles]
        positions = accessor(vertices, 'f', 5126, 'VEC3', 34962, True)
        normals = accessor(normals_for(vertices, triangles), 'f', 5126, 'VEC3', 34962)
        small_indices = len(vertices) <= 65536
        indices = accessor([(index,) for triangle in triangles for index in triangle],
                           'H' if small_indices else 'I', 5123 if small_indices else 5125,
                           'SCALAR', 34963)
        material = {'name': name, 'doubleSided': True,
                    'pbrMetallicRoughness': {'baseColorFactor': linear_color(color) + [1.0],
                                             'metallicFactor': 0.0, 'roughnessFactor': 0.75}}
        gltf['materials'].append(material)
        mesh_index = len(gltf['meshes'])
        gltf['meshes'].append({'name': name, 'primitives': [{
            'attributes': {'POSITION': positions, 'NORMAL': normals},
            'indices': indices, 'material': len(gltf['materials']) - 1,
        }]})
        gltf['nodes'].append({'name': name, 'mesh': mesh_index, 'extras': {
            'system': system, 'layer': layer, 'concept': concept,
        }})
        gltf['scenes'][0]['nodes'].append(len(gltf['nodes']) - 1)
        part_stats.append({'name': name, 'system': system, 'layer': layer, 'concept': concept,
                           'vertices': len(vertices), 'triangles': len(triangles)})

    while len(binary) % 4:
        binary.append(0)
    gltf['buffers'] = [{'byteLength': len(binary)}]
    encoded = json.dumps(gltf, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
    encoded += b' ' * (-len(encoded) % 4)
    glb = (struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(encoded) + 8 + len(binary))
           + struct.pack('<II', len(encoded), 0x4E4F534A) + encoded
           + struct.pack('<II', len(binary), 0x004E4942) + binary)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(glb)
    manifest = {
        'copyright': COPYRIGHT,
        'license': 'https://dbarchive.biosciencedbc.jp/en/bodyparts3d/lic.html',
        'sourceArchive': {'url': SOURCE_URL + 'partof_BP3D_4.0_obj_99.zip', 'sha256': digest(archive_path)},
        'sourceElements': {'url': SOURCE_URL + 'partof_element_parts.txt', 'sha256': digest(elements_path)},
        'sourceMeshNotices': sorted(original_headers),
        'transform': {'sourceCenter': source_center, 'scale': scale, 'xSign': x_sign,
                      'anteriorSign': anterior_sign, 'reverseWinding': reverse_winding},
        'output': {'file': output.name, 'sha256': hashlib.sha256(glb).hexdigest(), 'bytes': len(glb),
                   'vertices': sum(part['vertices'] for part in part_stats),
                   'triangles': sum(part['triangles'] for part in part_stats)},
        'parts': part_stats,
    }
    output.with_suffix('.provenance.json').write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(manifest['output']))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('elements', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    arguments = parser.parse_args()
    generate(arguments.archive, arguments.elements, arguments.output)
