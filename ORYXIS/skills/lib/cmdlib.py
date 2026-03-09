import ctypes
import json
import os

_DLL_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'dll')
_DLL_PATH = os.path.join(_DLL_DIR, 'cmdlib.dll')

if not os.path.exists(_DLL_PATH):
    raise FileNotFoundError(f"cmdlib.dll not found at {_DLL_PATH}")

_LIB = ctypes.CDLL(_DLL_PATH)

_LIB.run_command.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
_LIB.run_command.restype = ctypes.c_char_p

def run_command(cmd, args):
    if not isinstance(cmd, str):
        return json.dumps({"error": "cmd must be string"})
    if not isinstance(args, list):
        args = [str(args)]
    cmd_bytes = cmd.encode('utf-8')
    args_json = json.dumps(args, ensure_ascii=False).encode('utf-8')
    ptr = _LIB.run_command(cmd_bytes, args_json)
    return ptr.decode('utf-8') if ptr else '{"error":"null response"}'
