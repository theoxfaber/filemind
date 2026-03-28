import json
import time
import google.generativeai as genai
from config import GEMINI_API_KEY

genai.configure(api_key=GEMINI_API_KEY)
model = genai.GenerativeModel('gemini-flash-latest')

def _call_gemini_json(prompt: str) -> dict:
    response = model.generate_content(prompt)
    text = response.text.strip()
    # Sometimes Gemini wraps JSON in markdown blocks
    if text.startswith("```json"):
        text = text[7:]
    if text.startswith("```"):
        text = text[3:]
    if text.endswith("```"):
        text = text[:-3]
    return json.loads(text.strip())

def classify(filename: str, text: str) -> dict:
    """
    Classifies the document using Gemini API based on filename and extracted text.
    Returns a dict with 'category', 'confidence', and 'reasoning'.
    If confidence < 40, overrides category to 'Needs Review'.
    """
    time.sleep(0.5)  # Respect rate limits
    
    prompt = f"""You are an expert file organization agent. 
Analyze the filename and extracted text to determine the most logical category for this file.
If the extracted text is empty or unclear, rely heavily on the filename and extension.

### Examples:
Input: Filename: "invoice_123.pdf", Text: "Billed to: John Doe, Total: $50, Date: 2024-01-01"
Output: {{"category": "Invoices", "confidence": 100, "reasoning": "Standard invoice layout with billing and amount details."}}

Input: Filename: "vacation_photo.jpg", Text: ""
Output: {{"category": "Photos", "confidence": 90, "reasoning": "Image file extension and descriptive filename suggest a photograph."}}

Input: Filename: "script.py", Text: "import os\nprint('hello')"
Output: {{"category": "Code", "confidence": 100, "reasoning": "Python source code file with clear import statements."}}

Input: Filename: "medical_report.pdf", Text: "Patient: Alice, Symptoms: Headache, Diagnosis: Flu"
Output: {{"category": "Medical", "confidence": 95, "reasoning": "Document contains patient and medical diagnostic information."}}

Input: Filename: "xyz_random.dat", Text: "binary data 010101"
Output: {{"category": "Misc", "confidence": 30, "reasoning": "Unknown file type and binary content make classification difficult."}}

Now analyze the following:
Filename: {filename}
Extracted Text:
{text[:2000]}

Respond with a confidence score of 0-100.
Respond ONLY with valid JSON:
{{"category": "<Category Name>", "confidence": <int>, "reasoning": "<Concise explanation>"}}
"""

    try:
        return _call_gemini_json(prompt)
    except Exception as e:
        print(f"First classification attempt failed for {filename}: {e}")
        # Retry once with stricter prompt
        try:
            time.sleep(1.0) # wait a bit before retry
            strict_prompt = prompt + "\n\nCRITICAL: YOUR ENTIRE RESPONSE MUST BE VALID PARSABLE JSON. NO OTHER TEXT."
            return _call_gemini_json(strict_prompt)
        except Exception as e2:
            print(f"Second classification retry failed for {filename}: {e2}")
            return {
                "category": "Unclassified",
                "confidence": 0,
                "reasoning": "Classification failed due to parsing or API errors."
            }
