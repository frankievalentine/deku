import os

from fastapi import FastAPI

app = FastAPI(title="Deku FastAPI Starter")


@app.get("/")
async def index():
    return {
        "app": "fastapi",
        "message": "Deku FastAPI starter",
        "database_url_configured": bool(os.environ.get("DATABASE_URL")),
    }


@app.get("/health")
async def health():
    return {"status": "ok"}
