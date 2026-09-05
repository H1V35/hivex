export class HivexError extends Error {
  readonly code: string;
  readonly details: Readonly<Record<string, unknown>> | undefined;

  constructor(options: {
    code: string;
    message: string;
    details?: Readonly<Record<string, unknown>>;
  }) {
    super(options.message);
    this.name = 'HivexError';
    this.code = options.code;
    this.details = options.details;
  }
}
