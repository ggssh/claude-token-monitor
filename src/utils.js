// ── Formatting utilities ──

function fmt(n) {
  if (n === undefined || n === null) return '--';
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'K';
  return n.toLocaleString();
}

function fmtPct(n) {
  if (n === undefined || n === null) return '--';
  return n.toFixed(1) + '%';
}

function fmtCost(n) {
  if (!n || n <= 0) return '$0';
  if (n < 0.01) return '<$0.01';
  if (n < 1) return '$' + n.toFixed(3);
  return '$' + n.toFixed(2);
}
