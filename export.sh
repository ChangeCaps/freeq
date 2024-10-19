#!/bin/sh

cargo build --release

for path in $LD_LIBRARY_PATH; do
    patchelf --add-rpath $path target/release/libfreeq.so
done

mkdir -p ~/.vst3/FreeQ.vst3/Contents/x86_64-linux
cp target/release/libfreeq.so ~/.vst3/FreeQ.vst3/Contents/x86_64-linux/FreeQ.so
