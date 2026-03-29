<div align="center">
  
# 🧠 FileMind AI Organizer

**The intelligent, content-aware file organization system powered by AI.**

[![FastAPI](https://img.shields.io/badge/FastAPI-005571?style=for-the-badge&logo=fastapi)](https://fastapi.tiangolo.com/)
[![Google Gemini](https://img.shields.io/badge/Google%20Gemini-8E75B2?style=for-the-badge&logo=google&logoColor=white)](https://deepmind.google/technologies/gemini/)
[![Python 3.10+](https://img.shields.io/badge/Python-3.10+-3776AB?style=for-the-badge&logo=python&logoColor=white)](https://python.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

*Transform your digital chaos into structured clarity with zero effort.*

[Key Features](#-key-features) • [How It Works](#-how-it-works) • [Installation](#-installation) • [Usage](#-usage)

---

</div>

## 📖 About FileMind

**FileMind** is a premium, high-performance file organization tool designed to automatically analyze and categorize your documents, images, and text files. Instead of relying on file names or extensions, FileMind looks *inside* your files. 

By combining powerful text extraction (OCR) with advanced AI classification (Google Gemini), FileMind understands context, intelligently renames files, prevents duplicates, and moves them into a structured folder hierarchy perfectly tailored to your data.

## ✨ Key Features

- **🔍 Deep Content Analysis**: Extracts text from PDFs, images, and raw text files to understand what the file is actually about.
- **🧠 AI-Powered Classification**: Leverages cutting-edge LLMs with few-shot prompting to accurately categorize receipts, invoices, notes, reports, and more.
- **✨ Smart Renaming**: Forget `document_final_v2.pdf`. FileMind automatically generates semantic, descriptive filenames based on the file's core content.
- **🛡️ Intelligent De-Duplication**: Built-in MD5 hashing prevents processing the same file twice, even across multiple sessions.
- **💎 Premium Glassmorphism UI**: A stunning, responsive dark-themed dashboard with isometric animations to monitor the organization process in real-time.
- **📦 Seamless Export**: Download your cleanly grouped files as a `.zip` archive or directly sync them to a local directory.

---

## 🚀 How It Works

1. **Upload**: Drag & drop your messy files into the FileMind web interface.
2. **Extract**: The system rapidly scans structure and extracts raw text using advanced OCR and parser integrations.
3. **Classify**: Google Gemini AI evaluates the extracted text and assigns precise classification tags and confidence scores.
4. **Organize**: Files get cleanly renamed and sorted into perfectly structured subfolders. 

---

## 🛠 Installation

### Prerequisites
- Python 3.10 or higher
- An active API key for Google Gemini (Set up as an environment variable)

### Quick Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/theoxfaber/filemind.git
   cd filemind
   ```

2. **Create and activate a virtual environment:**
   ```bash
   python -m venv venv
   source venv/bin/activate  # On Windows: venv\Scripts\activate
   ```

3. **Install dependencies:**
   ```bash
   pip install -r requirements.txt
   ```

4. **Environment Variables:**
   Create a `.env` file in the root directory and add your keys:
   ```env
   GEMINI_API_KEY=your_api_key_here
   ```

5. **Start the FastAPI Server:**
   ```bash
   uvicorn main:app --reload
   ```
   
   *Your server will start on `http://localhost:8000`.*

---

## 🖥 Usage

Head over to the web app (`http://localhost:8000`), upload a batch of unorganized files, and watch FileMind's AI classify, rename, and organize them in real-time. Once the pipeline finishes, hit **Download Zip** or use the **Sync to Local Path** tool to move them to your desired directory.

---

> *"The best file manager is the one you never have to manage."* 🚀

<div align="center">
  <br/>
  <sub>Built with ❤️ by <a href="https://github.com/theoxfaber">Theoxfaber</a></sub>
</div>
