import { test, expect, type Page } from '@playwright/test';

const API_URL = process.env.VITE_API_URL || 'http://localhost:8080';

const INPUT = 'input[placeholder="What needs to be done?"]';

function uniqueTitle(prefix: string) {
	return `${prefix} ${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
}

async function rpc(fn: string, args: unknown = null) {
	const res = await fetch(`${API_URL}/_api/rpc/${fn}`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ args })
	});
	const json = await res.json();
	return json.data;
}

async function deleteAllTodos() {
	const todos = await rpc('list_todos');
	for (const todo of todos) {
		await rpc('delete_todo', { id: todo.id });
	}
}

// Navigate and wait for the SSE subscription to be fully registered.
// The subscribe POST only fires after the SSE connection is established,
// so this guarantees live updates will work before we interact with the page.
async function gotoReady(page: Page) {
	const subscribed = page.waitForResponse(
		(res) => res.url().includes('/_api/subscribe') && res.status() === 200
	);
	await page.goto('/');
	await subscribed;
}

test.beforeEach(async () => {
	await deleteAllTodos();
});

test.afterEach(async () => {
	await deleteAllTodos();
});

test.describe('smoke', () => {
	test('page loads with heading visible', async ({ page }) => {
		const errors: string[] = [];
		page.on('pageerror', (err) => errors.push(err.message));

		await gotoReady(page);
		await expect(page.locator('h1')).toHaveText('Todos');
		await expect(page.locator(INPUT)).toBeVisible();
		expect(errors).toHaveLength(0);
	});

	test('backend health check responds OK', async () => {
		const res = await fetch(`${API_URL}/_api/health`);
		expect(res.ok).toBe(true);
	});
});

test.describe('CRUD with reactivity', () => {
	test('create todo via button click', async ({ page }) => {
		const title = uniqueTitle('click-add');
		await gotoReady(page);

		await page.fill(INPUT, title);
		await page.click('.input-row button');

		await expect(page.locator('.title', { hasText: title })).toBeVisible({ timeout: 5000 });
	});

	test('create todo via Enter key', async ({ page }) => {
		const title = uniqueTitle('enter-add');
		await gotoReady(page);

		await page.fill(INPUT, title);
		await page.press(INPUT, 'Enter');

		await expect(page.locator('.title', { hasText: title })).toBeVisible({ timeout: 5000 });
	});

	test('toggle completion applies strikethrough', async ({ page }) => {
		const title = uniqueTitle('toggle');
		await gotoReady(page);

		await page.fill(INPUT, title);
		await page.click('.input-row button');
		await expect(page.locator('.title', { hasText: title })).toBeVisible({ timeout: 5000 });

		const todoItem = page.locator('li', { hasText: title });
		await todoItem.locator('input[type="checkbox"]').check();

		await expect(todoItem).toHaveClass(/completed/, { timeout: 5000 });
	});

	test('delete removes todo from list', async ({ page }) => {
		const title = uniqueTitle('delete');
		await gotoReady(page);

		await page.fill(INPUT, title);
		await page.click('.input-row button');
		await expect(page.locator('.title', { hasText: title })).toBeVisible({ timeout: 5000 });

		await page.locator('li', { hasText: title }).locator('button.delete').click();

		await expect(page.locator('.title', { hasText: title })).not.toBeVisible({ timeout: 5000 });
	});

	test('add button disabled when input empty', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('.input-row button')).toBeDisabled();
	});
});

test.describe('reactivity', () => {
	test('remaining count updates on add and complete', async ({ page }) => {
		const title = uniqueTitle('count');
		await gotoReady(page);

		await expect(page.locator('.status', { hasText: 'No todos yet' })).toBeVisible({
			timeout: 5000
		});

		await page.fill(INPUT, title);
		await page.click('.input-row button');
		await expect(page.locator('.count')).toHaveText('1 remaining', { timeout: 5000 });

		const todoItem = page.locator('li', { hasText: title });
		await todoItem.locator('input[type="checkbox"]').check();
		await expect(page.locator('.count')).toHaveText('0 remaining', { timeout: 5000 });
	});

	test('multiple rapid adds all appear', async ({ page }) => {
		const titles = [uniqueTitle('rapid-1'), uniqueTitle('rapid-2'), uniqueTitle('rapid-3')];
		await gotoReady(page);

		for (const title of titles) {
			await page.fill(INPUT, title);
			await page.click('.input-row button');
			await expect(page.locator(INPUT)).toHaveValue('', { timeout: 5000 });
		}

		for (const title of titles) {
			await expect(page.locator('.title', { hasText: title })).toBeVisible({ timeout: 5000 });
		}

		await expect(page.locator('.count')).toHaveText('3 remaining');
	});
});
