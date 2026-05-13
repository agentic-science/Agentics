import type { NextConfig } from "next";
import createNextIntlPlugin from "next-intl/plugin";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");

const defaultBackendOrigin = `http://127.0.0.1:${process.env.AGENTICS_API_PORT ?? "3100"}`;
const backendOrigin = (
  process.env.AGENTICS_API_BASE_URL ?? defaultBackendOrigin
).replace(/\/$/, "");

const nextConfig: NextConfig = {
  reactCompiler: true,
  allowedDevOrigins: ["127.0.0.1"],
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: `${backendOrigin}/api/:path*`,
      },
      {
        source: "/admin-api/:path*",
        destination: `${backendOrigin}/admin/:path*`,
      },
    ];
  },
};

export default withNextIntl(nextConfig);
