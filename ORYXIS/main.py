import requests
from bs4 import BeautifulSoup
import pdfkit
import os

# User Agent string
user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.3"

# Wikipedia URL for Rust
url = "https://en.wikipedia.org/wiki/Rust_(programming_language)"

# Fetch the content
response = requests.get(url, headers={"User-Agent": user_agent})
response.raise_for_status()

# Save to a PDF file on the desktop
desktop = os.path.join(os.path.expanduser("~"), "Desktop")
filename = "rust_programming_language.pdf"
file_path = os.path.join(desktop, filename)

# Convert HTML to PDF
pdfkit.from_url(url, file_path)

print("PDF saved to: " + file_path)