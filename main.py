import os
import json
import uuid
import shutil
from fastapi import FastAPI, UploadFile, File, BackgroundTasks, Request, HTTPException
from fastapi.responses import HTMLResponse, JSONResponse, FileResponse
from fastapi.templating import Jinja2Templates
from fastapi.middleware.cors import CORSMiddleware
from typing import List
from pydantic import BaseModel

from extractor import extract_text
from classifier import classify
from organizer import organize_file, MANIFEST_PATH, create_zip, sync_to_local, OUTPUT_DIR, get_file_hash

app = FastAPI(title="FileMind")

# CORS setup
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

UPLOAD_DIR = "uploads"
os.makedirs(UPLOAD_DIR, exist_ok=True)

templates = Jinja2Templates(directory="templates")

# In-memory map from id -> metadata for processing
file_map = {}

class ProcessRequest(BaseModel):
    file_ids: List[str]
    use_smart_rename: bool = False

class SyncRequest(BaseModel):
    target_path: str

@app.get("/", response_class=HTMLResponse)
async def index(request: Request):
    return templates.TemplateResponse("index.html", {"request": request})

@app.post("/upload")
async def upload_files(files: List[UploadFile] = File(...)):
    results = []
    for f in files:
        file_id = str(uuid.uuid4())
        filepath = os.path.join(UPLOAD_DIR, f"{file_id}_{f.filename}")
        with open(filepath, "wb") as buffer:
            shutil.copyfileobj(f.file, buffer)
        
        size = os.path.getsize(filepath)
        file_hash = get_file_hash(filepath)
        
        # Check for existing organized file with same hash
        is_duplicate = False
        if os.path.exists(MANIFEST_PATH):
            with open(MANIFEST_PATH, "r", encoding="utf-8") as mf:
                try:
                    manifest = json.load(mf)
                    if any(item.get("hash") == file_hash for item in manifest):
                        is_duplicate = True
                except: pass

        file_map[file_id] = {
            "path": filepath,
            "filename": f.filename,
            "hash": file_hash
        }
        
        results.append({
            "id": file_id,
            "filename": f.filename,
            "size": size,
            "is_duplicate": is_duplicate
        })
    return JSONResponse(content=results)

def process_pipeline(file_ids: List[str], use_smart_rename: bool):
    """Background task to process files"""
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    
    for fid in file_ids:
        if fid not in file_map:
            continue
            
        f_meta = file_map[fid]
        filepath = f_meta["path"]
        filename = f_meta["filename"]
        
        # 1. Extract
        try:
            text = extract_text(filepath)
        except Exception as e:
            text = ""
            print(f"Extraction failed for {filename}: {e}")
        
        # 2. Classify
        if text or filename:
            result = classify(filename, text or "")
        else:
            result = {
                "category": "Unclassified",
                "confidence": 0,
                "reasoning": "Failed to extract text and filename is missing."
            }
            
        # 3. Organize
        organize_file(
            filepath, 
            filename, 
            result.get("category", "Unclassified"), 
            result.get("confidence", 0), 
            result.get("reasoning", ""),
            use_smart_rename=use_smart_rename
        )

@app.post("/process")
async def process_files(req: ProcessRequest, background_tasks: BackgroundTasks):
    # Do NOT clear manifest on new process call anymore, let it accumulate
    # This allows de-duplication to work across sessions
    os.makedirs(OUTPUT_DIR, exist_ok=True)
        
    # Start background processing
    background_tasks.add_task(process_pipeline, req.file_ids, req.use_smart_rename)
    
    return {"message": "Processing started", "file_ids": req.file_ids}

@app.get("/results")
async def get_results():
    if not os.path.exists(MANIFEST_PATH):
        return []
        
    try:
        with open(MANIFEST_PATH, "r", encoding="utf-8") as f:
            data = f.read()
            if not data:
                return []
            return json.loads(data)
    except Exception:
        return []

@app.get("/download")
async def download_zip():
    if not os.path.exists(OUTPUT_DIR) or not os.listdir(OUTPUT_DIR):
        raise HTTPException(status_code=400, detail="No organized files available for download.")
    
    zip_path = create_zip()
    return FileResponse(zip_path, filename="filemind_organized.zip", media_type="application/zip")

@app.post("/sync")
async def sync_output(req: SyncRequest):
    if not os.path.exists(OUTPUT_DIR) or not os.listdir(OUTPUT_DIR):
        raise HTTPException(status_code=400, detail="No organized files available for sync.")
    
    success = sync_to_local(req.target_path)
    if success:
        return {"message": f"Successfully synchronized to {req.target_path}"}
    else:
        raise HTTPException(status_code=500, detail="Failed to synchronize files to local path.")
