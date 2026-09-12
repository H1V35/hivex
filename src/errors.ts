interface HivexErrorOptions extends ErrorOptions {
  code: string;
  details?: Readonly<Record<string, unknown>>;
  message: string;
}

export class HivexError extends Error {
  readonly code: string;
  readonly details: Readonly<Record<string, unknown>> | undefined;

  constructor(options: HivexErrorOptions) {
    super(options.message, options);
    this.name = "HivexError";
    this.code = options.code;
    this.details = options.details;
  }
}
