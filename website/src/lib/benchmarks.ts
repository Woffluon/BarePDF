export interface BenchmarkMetrics {
  timestamp: string;
  environment: {
    os: string;
    processor: string;
    processorCount: number;
    fixture: string;
  };
  metrics: {
    idle: {
      privateBytesMB: number;
      workingSetMB: number;
      cpuPercent: number;
    };
    documentLoaded: {
      privateBytesMB: number;
      workingSetMB: number;
      idleCpuPercent: number;
    };
  };
}

export async function getLatestBenchmarks(): Promise<BenchmarkMetrics | null> {
  // Use Vite / Astro glob import to statically discover benchmark records at build time
  const modules = import.meta.glob<{ default: BenchmarkMetrics }>(
    '../../../docs/benchmarks/*.json',
    { eager: true }
  );

  const keys = Object.keys(modules).sort().reverse();
  if (keys.length === 0) {
    return null;
  }

  const latestModule = modules[keys[0]];
  if (!latestModule || !latestModule.default) {
    return null;
  }

  const data = latestModule.default;
  if (!data.metrics || !data.environment) {
    return null;
  }

  return data;
}
