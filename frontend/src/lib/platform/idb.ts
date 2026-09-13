export function idbRequest<T>(operation: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    operation.onsuccess = () => resolve(operation.result);
    operation.onerror = () => reject(operation.error ?? new Error('IndexedDB request failed.'));
  });
}

/** Await only IndexedDB requests inside body. Resolve only when the transaction commits. */
export function idbTransaction<T>(
  database: IDBDatabase,
  stores: string[],
  mode: IDBTransactionMode,
  body: (tx: IDBTransaction) => Promise<T>,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const tx = database.transaction(stores, mode);
    let value: T;
    let failure: unknown;
    tx.oncomplete = () => resolve(value);
    tx.onabort = () => reject(failure ?? tx.error ?? new Error('History transaction aborted.'));
    void body(tx).then(
      (result) => {
        value = result;
      },
      (error) => {
        failure = error;
        try {
          tx.abort();
        } catch {
          reject(error);
        }
      },
    );
  });
}
