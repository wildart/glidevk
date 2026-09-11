#!/bin/sh
set -eu
cd "$(dirname "$0")"
"${GLSLC:-glslc}" glide.vert -o glide_vert.spv
"${GLSLC:-glslc}" glide.frag -o glide_frag.spv
