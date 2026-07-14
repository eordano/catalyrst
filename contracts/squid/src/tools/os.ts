// converts seconds and nanoseconds to milliseconds
function secNSec2ms(secNSec: number | [number, number]) {
  if (Array.isArray(secNSec)) {
    return secNSec[0] * 1000 + secNSec[1] / 1000000;
  }
  return secNSec / 1000;
}

export function getCpuPercentage() {
  const startTime = process.hrtime();
  const startUsage = process.cpuUsage();

  // spin the CPU for 500 milliseconds
  const now = Date.now();
  let elapTime = process.hrtime(startTime);
  while (Date.now() - now < 500) elapTime = process.hrtime(startTime);
  const elapUsage = process.cpuUsage(startUsage);

  const elapTimeMS = secNSec2ms(elapTime);
  const elapUserMS = secNSec2ms(elapUsage.user);
  const elapSystMS = secNSec2ms(elapUsage.system);
  const cpuPercent = Math.round((100 * (elapUserMS + elapSystMS)) / elapTimeMS);

  return cpuPercent;
}

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkCpuUsageAndThrottle() {
  const cpuUsage = getCpuPercentage();

  if (cpuUsage > 90) {
    await delay(100); // Adjust the delay as needed
  }
}
