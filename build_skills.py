import subprocess
import shutil
import os

LIBS = [
    ("libraries_opensource/skill_lib", "skill_lib"),
    ("libraries_opensource/cmdlib", "cmdlib"),
    ("libraries_opensource/file_lib", "file_lib"),
]
DLL_DIR = os.path.join("ORYXIS", "skills", "dll")

def build():
    os.makedirs(DLL_DIR, exist_ok=True)
    for path, name in LIBS:
        if not os.path.isdir(path):
            print(f"[SKIP] {path} not found")
            continue
        print(f"[BUILD] {name}...")
        r = subprocess.run(["cargo", "build", "--release"], cwd=path)
        if r.returncode != 0:
            print(f"[FAIL] {name}")
            continue
        src = os.path.join(path, "target", "release", f"{name}.dll")
        if not os.path.exists(src):
            print(f"[FAIL] {src} not found")
            continue
        dst = os.path.join(DLL_DIR, f"{name}.dll")
        shutil.copy2(src, dst)
        print(f"[OK] {name} -> {dst}")

if __name__ == "__main__":
    build()
