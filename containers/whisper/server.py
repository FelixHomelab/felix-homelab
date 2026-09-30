"""站内语音转文字服务（OpenAI 兼容）。

- 模型在启动时加载（GPU 可用则 CUDA float16，否则 CPU int8 回退）；
- 模型文件缓存到 `WHISPER_MODEL_DIR`（默认 /models，挂命名卷）；
- `HF_ENDPOINT` 可用国内镜像加速下载（见 Containerfile）。

环境变量：
  WHISPER_MODEL         模型名或本地路径（默认 large-v3）
  WHISPER_DEVICE        cuda / cpu（默认 cuda，失败自动回退 cpu）
  WHISPER_COMPUTE       float16 / int8_float16 / int8（默认按设备选）
  WHISPER_MODEL_DIR     模型缓存目录（默认 /models）
"""

import os
import tempfile

from fastapi import FastAPI, File, Form, UploadFile
from faster_whisper import WhisperModel

MODEL = os.environ.get("WHISPER_MODEL", "large-v3")
DEVICE = os.environ.get("WHISPER_DEVICE", "cuda")
COMPUTE = os.environ.get(
    "WHISPER_COMPUTE", "float16" if DEVICE == "cuda" else "int8"
)
MODEL_DIR = os.environ.get("WHISPER_MODEL_DIR", "/models")

app = FastAPI(title="Wraindrock STT")


def load_model():
    try:
        return WhisperModel(
            MODEL, device=DEVICE, compute_type=COMPUTE, download_root=MODEL_DIR
        ), DEVICE, COMPUTE
    except Exception as error:  # noqa: BLE001 - GPU 不可用时回退 CPU
        if DEVICE != "cpu":
            print(f"[stt] {DEVICE} 不可用（{error}），回退 CPU int8", flush=True)
            return (
                WhisperModel(
                    MODEL,
                    device="cpu",
                    compute_type="int8",
                    download_root=MODEL_DIR,
                ),
                "cpu",
                "int8",
            )
        raise


model, active_device, active_compute = load_model()
print(
    f"[stt] 模型就绪: {MODEL} device={active_device} compute={active_compute}",
    flush=True,
)


@app.get("/healthz")
def healthz():
    return {
        "status": "ok",
        "model": MODEL,
        "device": active_device,
        "compute": active_compute,
    }


@app.post("/v1/audio/transcriptions")
async def transcribe(
    file: UploadFile = File(...),
    language: str | None = Form(None),
    response_format: str = Form("json"),
):
    payload = await file.read()
    with tempfile.NamedTemporaryFile(suffix=".audio", delete=False) as tmp:
        tmp.write(payload)
        path = tmp.name
    try:
        segments, info = model.transcribe(
            path, language=language, vad_filter=True, beam_size=5
        )
        text = "".join(segment.text for segment in segments).strip()
        if response_format == "text":
            return text
        return {
            "text": text,
            "language": info.language,
            "duration": info.duration,
        }
    finally:
        os.unlink(path)
