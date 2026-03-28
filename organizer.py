import os
import shutil
import json
import hashlib
import zipfile
from datetime import datetime

OUTPUT_DIR = "output"
MANIFEST_PATH = os.path.join(OUTPUT_DIR, "manifest.json")

def get_file_hash(filepath: str) -> str:
    """Calculates MD5 hash of a file."""
    hasher = hashlib.md5()
    with open(filepath, 'rb') as f:
        for chunk in iter(lambda: f.read(4096), b""):
            hasher.update(chunk)
    return hasher.hexdigest()

def smart_rename(filename: str, category: str) -> str:
    """Generates a clean, systematic filename: YYYY-MM-DD - [Category] - [OriginalName]"""
    date_str = datetime.now().strftime("%Y-%m-%d")
    # Clean category (strip spaces, symbols)
    clean_cat = "".join(x for x in category if x.isalnum() or x in " -_").strip()
    return f"{date_str} - {clean_cat} - {filename}"

def organize_file(src_path: str, filename: str, category: str, confidence: int, reasoning: str, use_smart_rename: bool = False) -> dict:
    """
    Copies file to output/{category}/, appends to output/manifest.json.
    If confidence < 40, overrides category to 'Needs Review'.
    """
    # Confidence router rule: Lowered threshold to 40 for better auto-organization
    if confidence < 40:
        reasoning = f"[Low Confidence {confidence}%] " + reasoning
        category = "Needs Review"
        
    # Smart renaming
    final_filename = filename
    if use_smart_rename and category != "Needs Review":
        final_filename = smart_rename(filename, category)
        
    # Create output directories
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    category_dir = os.path.join(OUTPUT_DIR, category)
    os.makedirs(category_dir, exist_ok=True)
    
    # Copy file safely
    dest_path = os.path.join(category_dir, final_filename)
    shutil.copy2(src_path, dest_path)
    
    # Prepare result record
    record = {
        "filename": filename,
        "final_filename": final_filename,
        "category": category,
        "confidence": confidence,
        "reasoning": reasoning,
        "timestamp": datetime.now().isoformat(),
        "hash": get_file_hash(src_path)
    }
    
    # Update manifest.json
    manifest = []
    if os.path.exists(MANIFEST_PATH):
        with open(MANIFEST_PATH, 'r', encoding='utf-8') as f:
            try:
                manifest = json.load(f)
            except json.JSONDecodeError:
                pass
                
    manifest.append(record)
    
    with open(MANIFEST_PATH, 'w', encoding='utf-8') as f:
        json.dump(manifest, f, indent=2)
        
    return record

def create_zip(zip_name: str = "filemind_organized.zip") -> str:
    """
    Zips the entire output directory and returns the path to the zip file.
    """
    zip_path = os.path.join(os.path.dirname(OUTPUT_DIR), zip_name)
    with zipfile.ZipFile(zip_path, 'w', zipfile.ZIP_DEFLATED) as zipf:
        for root, dirs, files in os.walk(OUTPUT_DIR):
            for file in files:
                file_path = os.path.join(root, file)
                arcname = os.path.relpath(file_path, OUTPUT_DIR)
                zipf.write(file_path, arcname)
    return zip_path

def sync_to_local(target_path: str) -> bool:
    """
    Moves/Copies the content of OUTPUT_DIR to a local target_path.
    """
    try:
        if not target_path.startswith('/'):
            target_path = os.path.join(os.path.expanduser('~'), target_path)
        
        if not os.path.exists(target_path):
            os.makedirs(target_path, exist_ok=True)
            
        for root, dirs, files in os.walk(OUTPUT_DIR):
            ext_root = root.replace(OUTPUT_DIR, target_path, 1)
            os.makedirs(ext_root, exist_ok=True)
            for file in files:
                src_file = os.path.join(root, file)
                dest_file = os.path.join(ext_root, file)
                shutil.copy2(src_file, dest_file)
        return True
    except Exception as e:
        print(f"Sync failed: {e}")
        return False
