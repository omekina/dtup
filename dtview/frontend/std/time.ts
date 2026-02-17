import type { Debug, Display, Write } from "./fmt";
import type { f64, u32 } from "./mem";

export class Duration implements Debug, Display {
	private seconds: f64;

	public constructor(seconds: f64) {
		this.seconds = seconds;
	}

	public get floating_seconds(): f64 {
		return this.seconds;
	}

	public static from_millis(millis: u32): Duration {
		return new Duration(millis / 1000);
	}

	public static from_secs(secs: u32): Duration {
		return new Duration(secs);
	}

	public static from_mins(mins: u32): Duration {
		return new Duration(mins * 60);
	}

	public static from_hours(hours: u32): Duration {
		return new Duration(hours * 3600);
	}

	public static from_days(days: u32): Duration {
		return new Duration(days * 24 * 3600);
	}

	public add(other: Duration): Duration {
		return new Duration(this.seconds + other.seconds);
	}

	public sub(other: Duration): Duration {
		return new Duration(this.seconds - other.seconds);
	}

	private human_readable(): string {
		let seconds = this.seconds;
		let res = [];
		if (seconds >= 24 * 3600) {
			res.push(String(Math.floor(seconds / 3600 / 24)) + "d");
			seconds = seconds % (3600 * 24);
		}
		if (seconds >= 3600) {
			res.push(String(Math.floor(seconds / 3600)) + "h");
			seconds = seconds % 3600;
		}
		if (seconds >= 60) {
			res.push(String(Math.floor(seconds / 60)) + "m");
			seconds = seconds % 60;
		}
		if (seconds > 0) {
			res.push(String(seconds) + "s");
		}
		return res.join(" ");
	}

	fmt_dbg(writer: Write<string>): void {
		writer.write("Duration(");
		writer.write(this.human_readable());
		writer.write(")");
	}

	fmt_display(writer: Write<string>): void {
	    writer.write(this.human_readable());
	}
}

export function sleep(duration: Duration): Promise<void> {
	return new Promise((r) => {
		setTimeout(r, duration.floating_seconds * 1000);
	});
}
