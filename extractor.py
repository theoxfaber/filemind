import os
from PIL import Image
import pytesseract
import fitz  # PyMuPDF

MAX_CHARS = 2000

def extract_text(filepath: str) -> str:
    """
    Extracts text from the given file.
    Supports .txt, .md, images (.jpg, .jpeg, .png), and .pdf.
    Returns the first 2000 characters of the extracted text.
    On failure, returns an empty string.
    """
    try:
        ext = filepath.lower().split('.')[-1]
        text = ""
        
        if ext in ['txt', 'md']:
            with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
                text = f.read()
        elif ext in ['jpg', 'jpeg', 'png']:
            img = Image.open(filepath)
            text = pytesseract.image_to_string(img)
        elif ext == 'pdf':
            doc = fitz.open(filepath)
            for page in doc:
                text += page.get_text()
                if len(text) >= MAX_CHARS:
                    break
            
            # If native text extraction is empty, fallback to OCR on rasterized pages
            if not text.strip():
                for page_num in range(len(doc)):
                    page = doc.load_page(page_num)
                    pix = page.get_pixmap()
                    img = Image.frombytes("RGB", [pix.width, pix.height], pix.samples)
                    text += pytesseract.image_to_string(img)
                    if len(text) >= MAX_CHARS:
                        break
        else:
            # Unsupported format
            return ""
            
        return text[:MAX_CHARS]
    except Exception as e:
        print(f"Error extracting text from {filepath}: {e}")
        return ""
