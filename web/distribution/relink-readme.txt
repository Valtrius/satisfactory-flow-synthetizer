cvc5 browser module: source and relinking

This archive accompanies Satisfactory Flow Synthetizer's cvc5 1.3.4 browser
module. GMP 6.3.0 is used under the GNU Lesser General Public License, version
3 or later. Its notices and the GPL/LGPL license texts are under licenses/.
Other components keep their own licenses and notices.

Contents

sources/cvc5.tar.gz contains the exact unmodified cvc5 source commit.
sources/emsdk.tar.gz contains the pinned SDK installer source.
sources/GMP-EP.archive, CaDiCaL-EP.archive and SymFPU-EP.archive are original
source archives. Use tar -xf to extract them regardless of their suffix.
build.json records their URLs, hashes, compiler and configuration.
libraries/ contains every application/library object needed to relink.
session.o is the application wrapper object. session.cpp is its source.
relink.py links those objects without requiring the original working tree.

Relink on Linux x64 with Python 3.12+ and Emscripten 3.1.70 activated:

python3 relink.py --output relinked

To replace GMP with a modified compatible library built with the same SDK:

python3 relink.py --gmp-library /absolute/path/to/libgmp.a --output modified

The application's MIT license permits modifications and reverse engineering
for debugging such modifications. No signing key, activation service or
private relink material is required. These files carry no warranty.

For a full rebuild, extract the accompanying application source archive.
Its web/cvc5/toolchain.json and scripts/build-cvc5-wasm.py use the recorded
source pins. The builder downloads and verifies SDK/build dependencies into
a user cache, so an initial full rebuild needs internet access. Compiler
parallelism is two. Use the matching GMP source and its documentation to
produce a modified static library with the Emscripten toolchain.

To run a modified module in your own static build, replace the generated
cvc5.mjs and cvc5.wasm, update their byte counts and SHA-256 entries in the
generated build record, then rebuild the frontend. The frontend generates
new content-hashed URLs and an offline manifest. Those integrity checks are
not access controls and can be changed in the included application source.
Serve the rebuilt directory over localhost or HTTPS. Close existing app tabs
before switching an already installed offline version.
