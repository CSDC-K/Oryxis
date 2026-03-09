import ctypes
import json
import os

_DLL_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'dll')
_DLL_PATH = os.path.join(_DLL_DIR, 'skill_lib.dll')

if not os.path.exists(_DLL_PATH):
    raise FileNotFoundError(f"skill_lib.dll not found at {_DLL_PATH}")

_LIB = ctypes.CDLL(_DLL_PATH)

_LIB.get_skill_index.argtypes = [ctypes.c_char_p]
_LIB.get_skill_index.restype = ctypes.c_char_p
_LIB.get_yaml_content.argtypes = [ctypes.c_char_p]
_LIB.get_yaml_content.restype = ctypes.c_char_p
_LIB.get_all_index.argtypes = []
_LIB.get_all_index.restype = ctypes.c_char_p

def get_skill_index(tags):
    if not isinstance(tags, list):
        tags = [str(tags)]
    tags_json = json.dumps(tags, ensure_ascii=False).encode('utf-8')
    ptr = _LIB.get_skill_index(tags_json)
    if not ptr:
        return '[]'
    try:
        result = ptr.decode('utf-8')
        json.loads(result)  # validate
        return result
    except (UnicodeDecodeError, json.JSONDecodeError):
        return '[]'

def get_yaml_content(file_path):
    if not isinstance(file_path, str) or not file_path:
        return ''
    ptr = _LIB.get_yaml_content(file_path.encode('utf-8'))
    return ptr.decode('utf-8') if ptr else ''

def get_all_index():
    ptr = _LIB.get_all_index()
    if not ptr:
        return '[]'
    try:
        result = ptr.decode('utf-8')
        json.loads(result)  # validate
        return result
    except (UnicodeDecodeError, json.JSONDecodeError):
        return '[]'
