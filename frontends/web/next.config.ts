import type { NextConfig } from "next";
import createNextIntlPlugin from "next-intl/plugin";
import { loadAgenticsWebEnv } from "./src/lib/env";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");
const agenticsEnv = loadAgenticsWebEnv();

const nextConfig: NextConfig = {
  reactCompiler: true,
  allowedDevOrigins: [
    ...new Set(["127.0.0.1", "localhost", ...agenticsEnv.allowedDevOrigins]),
  ],
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: `${agenticsEnv.backendOrigin}/api/:path*`,
      },
      {
        source: "/admin-api/:path*",
        destination: `${agenticsEnv.backendOrigin}/admin/:path*`,
      },
    ];
  },
};

export default withNextIntl(nextConfig);
