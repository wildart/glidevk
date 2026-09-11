#!/usr/bin/env python3
"""Build with cargo build --workspace, then run these ABI/pixel/sync regressions."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--target-dir', type=Path, default=Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')))
args = parser.parse_args()
binaries = args.target_dir.resolve() / 'debug'
env = {**os.environ, 'VK_LAYER_ENABLES': 'VK_VALIDATION_FEATURE_ENABLE_SYNCHRONIZATION_VALIDATION_EXT'}
if sys.platform.startswith('linux'):
    env.setdefault('DISPLAY', ':1')
    env['WINIT_UNIX_BACKEND'] = 'x11'


def run(name, command, directory):
    result = subprocess.run(command, cwd=directory, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
    (directory / (name + '.log')).write_text(result.stdout)
    assert result.returncode == 0, f'{name} exited {result.returncode}:\n{result.stdout[-12000:]}'
    assert not re.search(r'\b(?:READ|WRITE)_AFTER_(?:READ|WRITE|PRESENT)\b', result.stdout), f'{name}: synchronization hazard\n{result.stdout}'
    print('PASS', name, flush=True)
    return result.stdout


with tempfile.TemporaryDirectory(prefix='glidevk-checks-') as temp:
    work = Path(temp)
    compiler = work / 'glslc-mock'
    compiler.write_text('#!/bin/sh\ncase "$1" in *.vert) exit 17;; *) exit 0;; esac\n')
    compiler.chmod(0o700)
    result = subprocess.run(['sh', str(ROOT / 'shaders/compile.sh')], env={**env, 'GLSLC': str(compiler)}, capture_output=True)
    assert result.returncode == 17, 'shader script hid a vertex compiler failure'
    print('PASS shader-script', flush=True)
    # C callers may use a 12-byte XY/PARGB vertex and noncanonical true values.
    if os.name != 'nt':
        library = binaries / ('libglide3x.dylib' if sys.platform == 'darwin' else 'libglide3x.so')
        child = r'''
import ctypes as c, mmap, resource, struct, sys
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
lib = c.CDLL(sys.argv[1])
lib.grGlideInit()
lib.grVertexLayout(1, 0, 1)
lib.grVertexLayout(0x30, 8, 1)
lib.grDepthMask(255)
lib.grFogMode(0x102)
lib.grTexCombine(0, 0x10, 8, 0x10, 8, 255, 255)
class TexInfo(c.Structure):
    _fields_ = [('small', c.c_int), ('large', c.c_int), ('aspect', c.c_int), ('format', c.c_int), ('data', c.c_void_p)]
info = TexInfo(0, 8, 0, 0x80, None)
lib.grTexSource.argtypes = [c.c_int, c.c_uint, c.c_uint, c.POINTER(TexInfo)]
lib.grTexSource(0, 0, 3, c.byref(info))
pages = mmap.mmap(-1, mmap.PAGESIZE * 2)
base = c.addressof(c.c_char.from_buffer(pages))
vertex = base + mmap.PAGESIZE - 12
c.memmove(vertex, struct.pack('<ffI', 10.0, 10.0, 0xffffffff), 12)
libc = c.CDLL(None)
libc.mprotect.argtypes = [c.c_void_p, c.c_size_t, c.c_int]
assert libc.mprotect(base + mmap.PAGESIZE, mmap.PAGESIZE, 0) == 0
for name, count in [('grDrawPoint', 1), ('grDrawLine', 2), ('grDrawTriangle', 3)]:
    draw = getattr(lib, name)
    draw.argtypes = [c.c_void_p] * count
    draw(*([vertex] * count))
lib.grGlideShutdown()
'''
        run('guard-page', [sys.executable, '-c', child, str(library)], work)
    expected = {
        'partial': [('BASELINE outside', 0x001f), ('BEFORE_SWAP front_pixel', 0x001f), ('PARTIAL outside', 0x001f)],
        'mixed': [('MIXED triangle', 0x07e0)],
        'frames': [('FRAMES completed', 10)],
        'state': [], 'textures': [], 'capacity': [], 'updates': [], 'samplers': [], 'locks-ui': [],
    }
    for mode, pixels in expected.items():
        output = run(mode, [str(binaries / 'review_probe'), mode], work)
        for label, expected_pixel in pixels:
            match = re.search(re.escape(label) + r'=(0x[0-9a-f]+|\d+)', output)
            assert match and int(match[1], 0) == expected_pixel, f'{mode}: {label}: {output}'
    for label, (width, height) in [('pillarbox', (1280, 720)), ('letterbox', (800, 800))]:
        directory = work / label
        directory.mkdir()
        (directory / 'glidevk.toml').write_text(f'[window]\nstatic_size = {{ custom = {{ width = {width}, height = {height} }} }}\nkeep_aspect_ratio = true\n')
        output = run(label, [str(binaries / 'review_probe'), 'scaled'], directory)
        for pixel in ['SCALED edge', 'SCALED center']:
            assert pixel + '=0xf800' in output, output
    ppm = work / 'clipping.ppm'
    run('clipping', [str(binaries / 'test15'), '-d', str(ppm)], work)
    magic, dimensions, maximum, pixels = ppm.read_bytes().split(b'\n', 3)
    assert (magic, dimensions, maximum) == (b'P6', b'640 480', b'255')
    offset = (36 * 640 + 16) * 3
    assert pixels[offset:offset + 3] == b'\0\0\0', 'pixel outside clip rectangle changed'
    for demo in ['test11', 'test16']:
        run(demo, [str(binaries / demo), '-n', '3'], work)
    print('All review regressions passed.')
