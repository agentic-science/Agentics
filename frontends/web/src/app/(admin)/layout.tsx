import { SiteChrome } from "@/components/SiteChrome";

/** Renders the admin layout component. */
export default async function AdminLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return <SiteChrome>{children}</SiteChrome>;
}
