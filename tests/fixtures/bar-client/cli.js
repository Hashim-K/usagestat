import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {providerBaseId} from './config.js';

const COMMAND_TIMEOUT_SECONDS = 90;

export function findAiUsage(override = '') {
    if (override)
        return GLib.file_test(override, GLib.FileTest.IS_EXECUTABLE) ? override : null;

    const paths = [
        GLib.getenv('USAGESTAT_CLI'),
        `${GLib.get_home_dir()}/.local/bin/usagestat`,
        '/home/linuxbrew/.linuxbrew/bin/usagestat',
        `${GLib.get_home_dir()}/.linuxbrew/bin/usagestat`,
        '/opt/homebrew/bin/usagestat',
        '/usr/local/bin/usagestat',
        '/usr/bin/usagestat',
    ].filter(Boolean);

    for (const path of paths) {
        if (GLib.file_test(path, GLib.FileTest.IS_EXECUTABLE))
            return path;
    }

    try {
        const proc = Gio.Subprocess.new(
            ['bash', '-lc', 'command -v usagestat'],
            Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE,
        );
        const [, stdout] = proc.communicate_utf8(null, null);
        const found = stdout.trim();
        if (found)
            return found;
    } catch {
        // Ignore PATH probe failures.
    }

    return null;
}

export function runAsync(argv, cancellable, timeoutMs = COMMAND_TIMEOUT_SECONDS * 1000) {
    return new Promise((resolve, reject) => {
        cancellable?.set_error_if_cancelled();
        const proc = Gio.Subprocess.new(
            argv,
            Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE,
        );

        let timedOut = false;
        let timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, timeoutMs, () => {
            timeoutId = 0;
            timedOut = true;
            try {
                proc.force_exit();
            } catch {
                // Process may have already exited.
            }
            return GLib.SOURCE_REMOVE;
        });
        const cancelId = cancellable?.connect(() => proc.force_exit());

        proc.communicate_utf8_async(null, cancellable, (process, result) => {
            if (timeoutId)
                GLib.source_remove(timeoutId);
            if (cancelId)
                cancellable.disconnect(cancelId);

            try {
                const [, stdout, stderr] = process.communicate_utf8_finish(result);
                if (timedOut)
                    throw new Error(`Command timed out after ${timeoutMs} ms.`);
                resolve({
                    stdout: stdout || '',
                    stderr: stderr || '',
                    status: process.get_if_exited() ? process.get_exit_status() : 128 + process.get_term_sig(),
                });
            } catch (error) {
                reject(error);
            }
        });
    });
}

export async function fetchProviderUsage(provider, cancellable, {cliPath = '', pluginDir = '', configFile = ''} = {}) {
    const providerId = providerBaseId(provider);
    pluginDir = pluginDir.trim();
    if (provider?.customCommand || provider?.custom === true || provider?.source === 'custom')
        return fetchCustomCommandUsage(provider, cancellable);

    const binary = findAiUsage(cliPath);
    if (!binary)
        throw new Error('usagestat CLI was not found on PATH or in common install locations.');

    const argv = [binary, '--json'];
    if (configFile)
        argv.push('--config', configFile);
    if (pluginDir)
        argv.push('--plugin-dir', pluginDir);
    argv.push('usage', '--provider', providerId);
    if (provider?.source && provider.source !== 'auto')
        argv.push('--source', provider.source);

    const result = await runAsync(argv, cancellable);

    const stdout = result.stdout.trim();
    if (!stdout) {
        const detail = result.stderr.trim().split('\n')[0] || `usagestat exited with status ${result.status}`;
        throw new Error(detail);
    }

    const payload = parseUsageJson(stdout, 'usagestat');
    if (payload?.error) {
        const message = payload.error.message || payload.error.code || JSON.stringify(payload.error);
        throw new Error(message);
    }
    const snapshot = normalizeBackendSnapshot(payload, providerId);
    try {
        const costSummary = await fetchProviderCostSummary(binary, providerId, cancellable, {pluginDir, configFile});
        if (costSummary) {
            snapshot.usage ||= {};
            snapshot.usage.costSummary = costSummary;
        }
    } catch {
        // Cost data is optional; keep live quota rendering usable if it is unavailable.
    }
    return snapshot;
}

export async function fetchProviderManifests(cancellable, {cliPath = '', pluginDir = '', configFile = ''} = {}) {
    pluginDir = pluginDir.trim();
    const binary = findAiUsage(cliPath);
    if (!binary)
        throw new Error('usagestat CLI was not found on PATH or in common install locations.');

    const argv = [binary, '--json'];
    if (configFile)
        argv.push('--config', configFile);
    if (pluginDir)
        argv.push('--plugin-dir', pluginDir);
    argv.push('list');

    const result = await runAsync(argv, cancellable);
    const stdout = result.stdout.trim();
    if (!stdout) {
        const detail = result.stderr.trim().split('\n')[0] || `usagestat exited with status ${result.status}`;
        throw new Error(detail);
    }

    const payload = JSON.parse(stdout);
    return Array.isArray(payload) ? payload : [];
}

async function fetchProviderCostSummary(binary, providerId, cancellable, {pluginDir = '', configFile = ''} = {}) {
    const argv = [binary, '--json'];
    if (configFile)
        argv.push('--config', configFile);
    if (pluginDir)
        argv.push('--plugin-dir', pluginDir);
    argv.push('cost', '--provider', providerId);

    const result = await runAsync(argv, cancellable);
    const stdout = result.stdout.trim();
    if (!stdout)
        return null;

    const payload = parseUsageJson(stdout, 'usagestat cost');
    return normalizeCostSummary(payload);
}

async function fetchCustomCommandUsage(provider, cancellable) {
    const command = provider.customCommand?.trim() || '';
    if (!command)
        throw new Error('Custom provider command is empty.');

    const result = await runAsync(['bash', '-lc', command], cancellable);
    const stdout = result.stdout.trim();
    if (!stdout) {
        const detail = result.stderr.trim().split('\n')[0] || `custom command exited with status ${result.status}`;
        throw new Error(detail);
    }

    const payload = parseUsageJson(stdout, 'custom command');
    if (payload?.error) {
        const message = payload.error.message || payload.error.code || JSON.stringify(payload.error);
        throw new Error(message);
    }
    return payload;
}

export function parseUsageJson(stdout, sourceName) {
    try {
        const parsed = JSON.parse(stdout);
        return Array.isArray(parsed) ? parsed[0] : parsed;
    } catch (error) {
        if (error instanceof SyntaxError)
            throw new Error(`Could not parse ${sourceName} JSON: ${error.message}`);
        throw error;
    }
}

export function normalizeBackendSnapshot(snapshot, fallbackProviderId) {
    if (!Array.isArray(snapshot?.metrics))
        return snapshot;

    const progress = [];
    const extraTextLines = [];
    const badges = [];

    for (const metric of snapshot.metrics) {
        if (metric?.type === 'progress') {
            const used = Number(metric.used) || 0;
            const limit = Number(metric.limit) || 0;
            if (limit <= 0)
                continue;
            progress.push({
                id: metric.label || `metric-${progress.length + 1}`,
                title: metric.label || null,
                window: {
                    label: metric.label || null,
                    usedPercent: Math.max(0, Math.min(100, used / limit * 100)),
                    used,
                    limit,
                    format: metric.format || null,
                    resetsAt: metric.resetsAt || null,
                    windowMinutes: metric.periodDurationMs ? Math.round(Number(metric.periodDurationMs) / 60000) : undefined,
                },
            });
        } else if (metric?.type === 'text') {
            if (isCostTextMetric(metric))
                continue;
            extraTextLines.push({
                label: metric.label || '',
                value: metric.value || '',
                subtitle: metric.subtitle || '',
            });
        } else if (metric?.type === 'badge') {
            badges.push({
                label: metric.label || '',
                text: metric.text || '',
                subtitle: metric.subtitle || '',
                color: metric.color || '',
            });
        }
    }

    const usage = {
        ...snapshot.usage,
        updatedAt: snapshot.fetchedAt || new Date().toISOString(),
        plan: snapshot.plan || null,
        extraTextLines,
        badges,
        extraRateWindows: [],
        providerCost: snapshot.providerCost || snapshot.usage?.providerCost || null,
    };

    for (const [index, tier] of ['primary', 'secondary', 'tertiary', 'quaternary'].entries()) {
        if (progress[index])
            usage[tier] = progress[index].window;
    }
    for (const item of progress.slice(4))
        usage.extraRateWindows.push(item);

    return {
        ...snapshot,
        provider: snapshot.providerId || fallbackProviderId,
        displayName: snapshot.displayName || null,
        source: snapshot.source || null,
        plan: snapshot.plan || null,
        usage,
        rawMetrics: snapshot.metrics,
        pace: snapshot.pace || null,
        statusPageUrl: snapshot.statusPageUrl || null,
        dashboardUrl: snapshot.usageDashboardUrl || snapshot.dashboardUrl || null,
    };
}

export function normalizeCostSummary(summary) {
    if (!summary || typeof summary !== 'object')
        return null;

    const currency = summary.currency || summary.currencyCode || 'USD';
    const daily = Array.isArray(summary.daily) ? summary.daily : [];
    const byDate = new Map(daily
        .filter(day => typeof day?.date === 'string')
        .map(day => [normalizeCostDate(day.date), day])
        .filter(([date]) => date));

    const today = localDateString(0);
    const yesterday = localDateString(-1);
    const totals = summary.totals || {};
    if (!hasCostData(totals) && !daily.some(hasCostData))
        return null;

    return {
        currency,
        lines: [
            costLine('Today', byDate.get(today), currency),
            costLine('Yesterday', byDate.get(yesterday), currency),
            costLine(`Last ${Number(summary.periodDays) || 30} Days`, totals, currency),
        ],
    };
}

function hasCostData(source) {
    return Number(source?.totalCost) > 0
        || Number(source?.totalTokens) > 0
        || Number(source?.inputTokens) > 0
        || Number(source?.outputTokens) > 0
        || Number(source?.cacheCreationTokens) > 0
        || Number(source?.cacheReadTokens) > 0;
}

function costLine(label, source, currency) {
    return {
        label,
        cost: Number(source?.totalCost) || 0,
        tokens: Number(source?.totalTokens) || 0,
        currency,
    };
}

function localDateString(offsetDays) {
    const date = GLib.DateTime.new_now_local().add_days(offsetDays);
    return date.format('%Y-%m-%d');
}

function normalizeCostDate(value) {
    if (/^\d{4}-\d{2}-\d{2}$/.test(value))
        return value;

    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime()))
        return '';
    const year = parsed.getFullYear();
    const month = String(parsed.getMonth() + 1).padStart(2, '0');
    const day = String(parsed.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
}

function isCostTextMetric(metric) {
    const label = String(metric?.label || '').toLowerCase();
    return label === 'today' || label === 'yesterday' || /^last \d+ days$/.test(label);
}
