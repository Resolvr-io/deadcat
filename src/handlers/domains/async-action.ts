export type AsyncActionOptions = {
  onError?: (error: unknown) => void;
  onFinally?: () => void;
};

export function runAsyncAction(
  action: () => Promise<void>,
  options?: AsyncActionOptions,
): void {
  void (async () => {
    try {
      await action();
    } catch (error) {
      if (options?.onError) {
        options.onError(error);
      } else {
        console.error("Unhandled async action error", error);
      }
    } finally {
      options?.onFinally?.();
    }
  })();
}
