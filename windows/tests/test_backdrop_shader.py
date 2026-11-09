"""Offline software-GL check for the exact backdrop shader strings in Rust.
Run with Python + moderngl + numpy installed. Does not need Servo or a website.
"""
from pathlib import Path
import re
import moderngl
import numpy as np

source = (Path(__file__).parents[1] / 'desktop' / 'backdrop.rs').read_text()
shaders = re.findall(r'\(glow::(?:VERTEX|FRAGMENT)_SHADER, "([^"]+)"\)', source)
assert len(shaders) == 2
ctx = moderngl.create_standalone_context(backend='egl', require=330)
program = ctx.program(vertex_shader='#version 150\n'+shaders[0], fragment_shader='#version 150\n'+shaders[1])
vao = ctx.vertex_array(program, [])
size = 96
pixels = np.zeros((size,size,4),dtype='u1')
pixels[:,:,3] = 255
pixels[:,47:49,:3] = 255
texture = ctx.texture((size,size),4,pixels.tobytes())
texture.filter = (moderngl.LINEAR,moderngl.LINEAR)
texture.repeat_x = texture.repeat_y = False
output = ctx.simple_framebuffer((size,size),components=4)
output.use()
output.clear(0,0,0,0)
texture.use(0)
program['source'] = 0
program['screen'] = (size,size)
program['region'] = (0,0,size,size)
vao.render(vertices=3)
result = np.frombuffer(output.read(components=4),dtype='u1').reshape(size,size,4)
# The hard 2px stripe must lose its peak and produce a surrounding halo.
assert 0 < result[48,48,0] < 220, result[48,48]
assert result[48,43,0] > 0, result[48,43]
assert result[0,0,3] == 0, 'rounded corner was not masked'
assert result[48,48,3] == 255
print('Exact backdrop shaders compiled and rendered: stripe blurred, alpha and rounded mask passed')
