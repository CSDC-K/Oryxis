import ctypes
import os

_DLL_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'dll')
_DLL_PATH = os.path.join(_DLL_DIR, 'file_lib.dll')

if not os.path.exists(_DLL_PATH):
    raise FileNotFoundError(f"file_lib.dll not found at {_DLL_PATH}")

_LIB = ctypes.CDLL(_DLL_PATH)

def _s(name, args, res=ctypes.c_char_p):
    f = getattr(_LIB, name); f.argtypes = args; f.restype = res; return f

_fns = {
    'read_file': _s('read_file', [ctypes.c_char_p]),
    'write_file': _s('write_file', [ctypes.c_char_p, ctypes.c_char_p]),
    'append_to_file': _s('append_to_file', [ctypes.c_char_p, ctypes.c_char_p]),
    'delete_file': _s('delete_file', [ctypes.c_char_p]),
    'move_file': _s('move_file', [ctypes.c_char_p, ctypes.c_char_p]),
    'copy_file': _s('copy_file', [ctypes.c_char_p, ctypes.c_char_p]),
    'list_directory': _s('list_directory', [ctypes.c_char_p]),
    'create_directory': _s('create_directory', [ctypes.c_char_p]),
    'delete_directory': _s('delete_directory', [ctypes.c_char_p]),
    'create_file': _s('create_file', [ctypes.c_char_p]),
    'path_exists': _s('path_exists', [ctypes.c_char_p]),
    'is_file': _s('is_file', [ctypes.c_char_p]),
    'is_directory': _s('is_directory', [ctypes.c_char_p]),
    'get_metadata': _s('get_metadata', [ctypes.c_char_p]),
    'get_file_hash': _s('get_file_hash', [ctypes.c_char_p]),
}

def _d(b): return b.decode('utf-8') if b else ''
def _b(s): return s.encode('utf-8') if isinstance(s, str) else str(s).encode('utf-8')

def _call1(fn, a):       return _d(_fns[fn](_b(a)))
def _call2(fn, a, b):    return _d(_fns[fn](_b(a), _b(b)))
def _bool1(fn, a):       return _d(_fns[fn](_b(a))) == 'true'

def read_file(path):             return _call1('read_file', path)
def write_file(path, content):   return _call2('write_file', path, content)
def append_to_file(path, c):     return _call2('append_to_file', path, c)
def delete_file(path):           return _call1('delete_file', path)
def move_file(src, dst):         return _call2('move_file', src, dst)
def copy_file(src, dst):         return _call2('copy_file', src, dst)
def list_directory(path):        return _call1('list_directory', path)
def create_directory(path):      return _call1('create_directory', path)
def delete_directory(path):      return _call1('delete_directory', path)
def create_file(path):           return _call1('create_file', path)
def path_exists(path):           return _bool1('path_exists', path)
def is_file(path):               return _bool1('is_file', path)
def is_directory(path):          return _bool1('is_directory', path)
def get_metadata(path):          return _call1('get_metadata', path)
def get_file_hash(path):         return _call1('get_file_hash', path)
