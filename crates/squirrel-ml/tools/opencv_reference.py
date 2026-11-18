"""OpenCV's own embeddings of the face test portraits, as an independent
reference for the Rust pipeline (YuNet → alignment → SFace).

Optional and run by hand: never part of the build or the tests. It needs
Python with `opencv-python` (cv2) and NumPy, and the pinned model files:

    python3 crates/squirrel-ml/tools/opencv_reference.py <repo> [models_dir]

It writes `crates/squirrel-ml/tests/fixtures/faces/opencv-reference.json`.
models_dir defaults to <repo>/.cache/squirrel-assets/models.
"""

import json
import sys

import cv2
import numpy as np

PORTRAITS = ["koch-a", "koch-b", "glover-a", "glover-b"]


def main() -> None:
    repo = sys.argv[1]
    models = (sys.argv[2] if len(sys.argv) > 2 else repo + "/.cache/squirrel-assets/models") + "/faces/"
    fixtures = repo + "/crates/squirrel-ml/tests/fixtures/faces/"
    recognizer = cv2.FaceRecognizerSF.create(models + "face_recognition_sface_2021dec.onnx", "")
    embeddings = {}
    for name in PORTRAITS:
        image = cv2.imread(fixtures + name + ".jpg")
        height, width = image.shape[:2]
        detector = cv2.FaceDetectorYN.create(
            models + "face_detection_yunet_2026may.onnx", "", (width, height), 0.6, 0.3, 5000
        )
        _, faces = detector.detect(image)
        if faces is None or len(faces) != 1:
            sys.exit(f"{name}: expected one face, got {0 if faces is None else len(faces)}")
        # alignCrop and feature take the BGR image (feature swaps to RGB itself).
        feature = recognizer.feature(recognizer.alignCrop(image, faces[0])).flatten()
        feature = feature / np.linalg.norm(feature)
        embeddings[name] = [round(float(v), 6) for v in feature]
    out = {
        "generator": "crates/squirrel-ml/tools/opencv_reference.py",
        "opencv": cv2.__version__,
        "faces": embeddings,
    }
    with open(fixtures + "opencv-reference.json", "w") as file:
        file.write(json.dumps(out) + "\n")
    print("wrote", fixtures + "opencv-reference.json", "with OpenCV", cv2.__version__)


if __name__ == "__main__":
    main()
