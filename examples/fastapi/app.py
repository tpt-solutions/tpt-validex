"""FastAPI request validation backed by tpt-validex."""

import json

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

from tpt_validex import Validator

app = FastAPI(title="tpt-validex FastAPI example")

USER_SCHEMA = {
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age": {"type": "integer", "minimum": 0, "maximum": 150},
        "email": {"type": "string", "format": "email"},
    },
    "required": ["name", "age"],
    "additionalProperties": False,
}
validator = Validator(USER_SCHEMA)


@app.post("/users")
async def create_user(request: Request) -> JSONResponse:
    raw = await request.body()
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as exc:
        return JSONResponse({"errors": [{"path": "$", "message": str(exc)}]}, status_code=400)

    is_valid, errors = validator.validate(payload)
    if not is_valid:
        return JSONResponse({"errors": errors}, status_code=422)

    return JSONResponse({"user": payload, "status": "created"})


@app.get("/health")
async def health() -> dict:
    return {"ok": True}
