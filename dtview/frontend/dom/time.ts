export function dom_refresh(): Promise<void> {
	return new Promise((r) => {
		requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				r();
			});
		});
	});
}
