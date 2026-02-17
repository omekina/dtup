import { format_debug, format_display, type Debug, type Display, type Write } from "./fmt";
import { Drop, Ref, Vec, Option, type bool } from "./mem";

export class Asyncify<T> {
	private resolvers: Vec<() => T>;
	private pollers: Vec<(value: T) => void>;
	private backlog: boolean;

	/**
	* If backlog is disabled, resolvers are not stacked, so some resolves may be lost.
	*/
	public constructor(backlog: boolean = false) {
		this.backlog = backlog;
		this.resolvers = new Vec();
		this.pollers = new Vec();
	}

	/**
	* Guaranteed to resolve immediately if backlog is disabled (although the value can be lost).
	*
	* If backlog is enabled, this will resolve once the value is received by a poller.
	*/
	public resolve(value: T): Promise<void> {
		if (this.pollers.len() > 0) {
			this.pollers.req_shift()(value);
			return Promise.resolve();
		} else if (this.backlog) {
			return new Promise((r) => {
				this.resolvers.push(() => { r(); return value; });
			});
		} else {
			// ignore the value if there are no waiters and backlog is disabled
			return Promise.resolve();
		}
	}

	public poll(): Promise<T> {
		if (this.resolvers.len() > 0) {
			return Promise.resolve(this.resolvers.req_shift()());
		}
		return new Promise((r) => {
			this.pollers.push(v => { r(v); });
		});
	}

	public has_backlog(): bool {
		return this.resolvers.len() > 0;
	}

	public has_pollers(): bool {
		return this.pollers.len() > 0;
	}
}

export class Mutex<T> implements Debug, Display {
	private value: Ref<T>;
	private waiters: Vec<() => void>;
	private locked: boolean;
	private poisoned: boolean;

	public constructor(value: T) {
		this.value = new Ref(value);
		this.waiters = new Vec();
		this.locked = false;
		this.poisoned = false;
	}

	private fail_poisoned(): never {
		throw new Error("poisoned mutex");
	}

	private sanity_check(): void {
		if (this.poisoned) {
			this.fail_poisoned();
		}
	}

	private poison(): void {
		this.poisoned = true;
		this.fail_poisoned();
	}

	private construct_guard(): MutexGuard<T> {
		return new MutexGuard(this.value, () => { this.unlock(); });
	}

	public lock(): Promise<MutexGuard<T>> {
		this.sanity_check();
		if (!this.locked) {
			this.locked = true;
			return Promise.resolve(this.construct_guard());
		}
		return new Promise((r) => {
			this.waiters.push(() => {
				r(new MutexGuard(this.value, () => { this.unlock(); }));
			});
		});
	}

	private unlock(): void {
		this.sanity_check();
		if (!this.locked) {
			this.poison();
		}
		if (this.waiters.len() > 0) {
			this.waiters.req_shift()();
		} else {
			this.locked = false;
		}
	}

	fmt_dbg(writer: Write<string>): void {
		writer.write("Mutex(");
		format_debug(this.value.v, writer);
		writer.write(")");
	}

	fmt_display(writer: Write<string>): void {
		format_display(this.value.v, writer);
	}
}

class MutexGuard<T> extends Drop implements Debug, Display {
	private value: Option<Ref<T>>;
	private unlock_callback: () => void;

	public constructor(value: Ref<T>, unlock_callback: () => void) {
		super();
		this.value = Option.some(value);
		this.unlock_callback = unlock_callback;
	}

	private require(): Ref<T> {
		if (this.value.is_some()) {
			throw new Error("mutex guard used after drop");
		}
		return this.value.deref();
	}

	public get deref(): T {
		return this.require().v;
	}

	public set deref(value: T) {
		this.require().v = value;
	}

	public drop(): void {
		this.value.set_none();
		this.unlock_callback();
	}

	fmt_dbg(writer: Write<string>): void {
		writer.write("MutexGuard(");
		if (this.value.is_some()) {
			format_debug(this.value.deref().v, writer);
		} else {
			writer.write("Dropped");
		}
		writer.write(")");
	}

	fmt_display(writer: Write<string>): void {
		format_display(this.value, writer);
	}
}

class Mpsc<T> {
	private consumer_dropped: bool;
	private producers: Vec<() => T>;
	private consumer: Option<(value: T) => void>;
	private backlog: bool;
	private endpoints_registered: bool;

	public constructor(backlog: bool) {
		this.consumer_dropped = false;
		this.backlog = backlog;
		this.consumer = Option.none();
		this.producers = new Vec();
		this.endpoints_registered = false;
	}

	private produce(value: T): Promise<void> {
		if (this.consumer.is_some()) {
			this.consumer.deref()(value);
			return Promise.resolve();
		} else if (this.backlog) {
			return new Promise((r) => {
				this.producers.push(() => { r(); return value; });
			});
		} else {
			return Promise.resolve();
		}
	}

	private consume(): Promise<T> {
		if (this.producers.len() > 0) {
			return Promise.resolve(this.producers.req_shift()());
		} else if (!this.consumer.is_some()) {
			return new Promise((r) => {
				this.consumer.set_some((v) => {
					this.consumer.set_none();
					return r(v);
				});
			});
		} else {
			throw new Error("registered multiple receivers on a mpsc channel");
		}
	}

	private drop_consumer() {
		if (this.consumer_dropped) {
			throw new Error("re-drop of mpsc receiver");
		}
		this.consumer_dropped = true;
	}

	public register(): [Sender<T>, Receiver<T>] {
		if (this.endpoints_registered) {
			throw new Error("re-registered endpoints on a mpsc channel");
		}
		this.endpoints_registered = true;
		const tx = new Sender<T>((v) => { return this.produce(v); }, () => {});
		const rx = new Receiver<T>(
			() => { return this.consume(); }, () => { this.drop_consumer(); }
		);
		return [tx, rx];
	}
}

export class Sender<T> extends Drop {
	private handler: (value: T) => Promise<void>;
	private drop_callback: () => void;

	public constructor(
		handler: (value: T) => Promise<void>,
		drop_callback: () => void,
	) {
		super();
		this.handler = handler;
		this.drop_callback = drop_callback;
	}

	public send(value: T): Promise<void> {
		return this.handler(value);
	}

	public override drop(): void {
	    this.drop_callback();
	}
}

export class Receiver<T> extends Drop {
	private handler: () => Promise<T>;
	private drop_callback: () => void;

	public constructor(
		handler: () => Promise<T>,
		drop_callback: () => void,
	) {
		super();
		this.handler = handler;
		this.drop_callback = drop_callback;
	}

	public receive(): Promise<T> {
		return this.handler();
	}

	public override drop(): void {
	    this.drop_callback();
	}
}

/**
* If backlog is enabled, senders will wait until the receiver takes the value.
* If backlog is disabled, senders will try to send and ignore the value if the receiver is not
* ready.
*/
export function mpsc<T>(backlog: bool): [Sender<T>, Receiver<T>] {
	return new Mpsc<T>(backlog).register();
}
