// This module runs beside the Emscripten instance inside one worker.
export function createSessionApi(module) {
  const create = module.cwrap('sfs_session_create', 'number', ['number']);
  const execute = module.cwrap('sfs_session_execute', 'number', ['number', 'number', 'number']);
  const output = module.cwrap('sfs_session_output', 'string', ['number']);
  const error = module.cwrap('sfs_session_error', 'string', []);
  const destroy = module.cwrap('sfs_session_destroy', 'number', ['number']);
  const activeSessions = module.cwrap('sfs_session_count', 'number', []);
  const heapBytes = module.cwrap('sfs_heap_bytes', 'number', []);
  return {
    activeSessions,
    heapBytes,
    createSession({ resourceLimit = 0 } = {}) {
      if (!Number.isInteger(resourceLimit) || resourceLimit < 0 || resourceLimit > 0xffffffff) {
        throw new Error('resourceLimit must be an unsigned 32-bit integer');
      }
      const id = create(resourceLimit);
      if (id === 0) throw new Error(error());
      let disposed = false;
      return {
        execute(commands) {
          if (disposed) throw new Error('cvc5 session is disposed');
          if (typeof commands !== 'string' || commands.includes('\0')) {
            throw new Error('SMT commands must be a string without NUL bytes');
          }
          const bytes = module.lengthBytesUTF8(commands);
          if (bytes > 16 * 1024 * 1024) throw new Error('SMT command batch exceeds 16 MiB');
          // cwrap's string arguments use the Wasm stack. Encodings can be larger than it.
          const pointer = module._malloc(bytes + 1);
          if (!pointer) throw new Error('unable to allocate SMT command batch');
          try {
            module.stringToUTF8(commands, pointer, bytes + 1);
            if (execute(id, pointer, bytes) !== 1) throw new Error(error());
            return output(id).trim();
          } finally {
            module._free(pointer);
          }
        },
        dispose() {
          if (!disposed) {
            destroy(id);
            disposed = true;
          }
        },
      };
    },
  };
}
