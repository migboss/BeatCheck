---
license: mit
library_name: onnx
tags:
- beat-tracking
- downbeat-tracking
- music
- onnx
---

# Beat This! (ONNX conversion)

ONNX files of **Beat This!** by Francesco Foscarin, Jan Schlüter and Gerhard Widmer (Institute of
Computational Perception, Johannes Kepler University Linz): "Beat This! Accurate Beat Tracking Without
DBN Postprocessing", ISMIR 2024. Original code and weights: https://github.com/CPJKU/beat_this
(MIT, see `LICENSE`).

These files are a format conversion of the published `final0` and `small0` checkpoints, made with
`tools/beat-this-export/export_beat_this.py` in the PansEcho repository (beat-this 1.1.0 from PyPI,
torch 2.15.0, onnx 1.23.2, onnxruntime 1.31.1). ONNX Runtime output matches PyTorch within 1e-3
(measured max abs difference about 2e-5). The PansEcho Mac app downloads them only when the user asks,
after showing this licence; they are not bundled with the app.

| File | Bytes | sha256 |
|---|---|---|
| `beat_this_final0.onnx` | 83162700 | `a7e2c68b27f9db6a73416017f784ea8551a01373f4955a98db018827760c4a14` |
| `beat_this_small0.onnx` | 10555642 | `53fda6e96afae7f44d898fa5f020272eda967a92839306bf7a3d0898fb8a0d9a` |
| `logmel.onnx` | 4468830 | `f5afa499f5a42b05ea89a214561170cd4b76f92d67d3227f7c9833e8d79bbb43` |

`logmel.onnx` is the front-end: 22050 Hz mono audio in, 128-band log-mel frames at 50 fps out (a fixed
filterbank, no trained weights). The beat models take up to 1500 frames per call and return beat and
downbeat logits per frame.

Training data note (from the authors): some of the training data contains copyrighted music or files
under limited Creative Commons terms; the weights are distributed by CPJKU as research artifacts.
