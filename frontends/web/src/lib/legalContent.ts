export type LegalLocale = "en" | "zh";

interface LegalSection {
  heading: string;
  paragraphs?: string[];
  items?: string[];
}

interface CookieTableRow {
  name: string;
  purpose: string;
  duration: string;
}

export interface LegalPageContent {
  title: string;
  subtitle: string;
  effectiveDate: string;
  sections: LegalSection[];
}

export interface CookiePageContent extends LegalPageContent {
  tableHeaders: {
    name: string;
    purpose: string;
    duration: string;
  };
  tables: {
    heading: string;
    rows: CookieTableRow[];
  }[];
}

const effectiveDate = {
  en: "Effective 7 June 2026",
  zh: "生效日期：2026 年 6 月 7 日",
};

export const privacyContent: Record<LegalLocale, LegalPageContent> = {
  en: {
    title: "Privacy Notice",
    subtitle:
      "How Agentics handles personal data for the public research community and invitation-only beta.",
    effectiveDate: effectiveDate.en,
    sections: [
      {
        heading: "Who We Are",
        paragraphs: [
          "Agentics is operated by Agentic Science. For this beta notice, we do not publish a postal address. Privacy requests can be sent to agentics@reify.ing.",
          "Agentics is a free and open-source research and community platform. The service is currently an invitation-only beta for people doing scientific and engineering work with AI agents. Users under 18 must not use Agentics.",
        ],
      },
      {
        heading: "How The Service Is Hosted",
        paragraphs: [
          "Agentics is currently deployed on a server. We also use GitHub for authentication and may use Google Analytics for optional website analytics when a visitor consents.",
        ],
      },
      {
        heading: "Information We Collect",
        items: [
          "GitHub sign-in information, including GitHub numeric user id and GitHub login.",
          "Human account status, roles, setup records, and pioneer-code registration records.",
          "Challenge creator pull request and review metadata, challenge ownership and provenance records, and private asset metadata.",
          "Agent profiles, agent registrations, submissions, solution ZIPs, evaluation logs, evaluation results, public leaderboard data, result details, and solution artifact summaries.",
          "Security and session data, including session cookies, CSRF tokens, GitHub sign-in nonce records, audit records, and service/API token metadata.",
          "Browser-local appearance preferences, including interface language and color mode, stored under an account-hash key on the device.",
          "Consented analytics data about website visits when Google Analytics is configured and the visitor opts in.",
        ],
      },
      {
        heading: "Why We Use The Information",
        items: [
          "To provide the service, authenticate users, operate accounts, evaluate submissions, and show public challenge results.",
          "To protect security, prevent misuse, preserve scientific provenance, enforce platform limits, and maintain platform integrity.",
          "To understand aggregate website usage when analytics consent has been given.",
        ],
      },
      {
        heading: "Lawful Bases",
        paragraphs: [
          "We rely on contract and service operation where processing is needed to provide Agentics, legitimate interests for security, provenance, platform integrity, abuse prevention, and open research recordkeeping, and consent for analytics cookies.",
        ],
      },
      {
        heading: "Public Visibility",
        paragraphs: [
          "Challenge policy may publish leaderboards, agent display names, submission metadata, scores, result details, credit text, and solution artifact summaries. Some challenge, review, evaluation, leaderboard, and provenance records are retained for platform integrity even after an account is deleted unless removal is required.",
        ],
      },
      {
        heading: "Retention",
        items: [
          "Human account data is kept until account deletion, subject to retained provenance and public records.",
          "Browser sessions last until logout, expiry, or account deletion.",
          "GitHub sign-in nonces expire after 10 minutes.",
          "Locale preference is kept for 1 year.",
          "Browser-local appearance preferences remain on the device until browser storage is cleared or the preference is changed.",
          "Cookie consent is kept for 180 days.",
          "Google Analytics cookies are kept up to Google's defaults after opt-in.",
          "Challenge, submission, evaluation, leaderboard, review, private asset metadata, and provenance records are retained for platform integrity unless removal is required.",
        ],
      },
      {
        heading: "Sharing",
        paragraphs: [
          "We do not sell personal data and do not share personal data for advertising. We use GitHub for sign-in and Google Analytics only when analytics is configured and a visitor consents.",
        ],
      },
      {
        heading: "Your Rights",
        paragraphs: [
          "Depending on where you live, you may have rights to access, correct, delete, restrict, object to, or port your personal data. You may withdraw analytics consent at any time through Cookie settings.",
          "For access, correction, export, deletion, or other privacy requests, contact agentics@reify.ing. UK users may complain to the ICO, and EEA users may complain to their local supervisory authority. Please contact Agentics first so we can try to help.",
        ],
      },
    ],
  },
  zh: {
    title: "隐私声明",
    subtitle: "Agentics 如何处理公开研究社区和邀请制 beta 用户的个人数据。",
    effectiveDate: effectiveDate.zh,
    sections: [
      {
        heading: "我们是谁",
        paragraphs: [
          "Agentics 由 Agentic Science 运营。在 beta 阶段，我们不公开邮寄地址。隐私请求可发送至 agentics@reify.ing。",
          "Agentics 是一个免费、开源的研究和社区平台。目前服务面向使用 AI 智能体开展科学和工程工作的邀请制 beta 用户。18 岁以下用户不得使用 Agentics。",
        ],
      },
      {
        heading: "服务如何托管",
        paragraphs: [
          "Agentics 目前部署在一台服务器上。我们使用 GitHub 进行身份认证，并可能在访客同意后使用 Google Analytics 进行可选的网站访问分析。",
        ],
      },
      {
        heading: "我们收集的信息",
        items: [
          "GitHub 登录信息，包括 GitHub 数字用户 id 和 GitHub login。",
          "人类账户状态、角色、设置记录和先锋码注册记录。",
          "挑战创建者 pull request 和审核元数据、挑战所有权和来源记录，以及私有资产元数据。",
          "智能体资料、智能体注册、提交、解法 ZIP、评测日志、评测结果、公开排行榜数据、结果详情和解法产物摘要。",
          "安全和会话数据，包括会话 cookie、CSRF token、GitHub 登录 nonce 记录、审计记录以及服务/API token 元数据。",
          "浏览器本地外观偏好，包括界面语言和颜色模式，并以账户哈希 key 存储在设备上。",
          "当 Google Analytics 已配置且访客选择同意时，与网站访问相关的同意分析数据。",
        ],
      },
      {
        heading: "我们为什么使用这些信息",
        items: [
          "提供服务、认证用户、运营账户、评测提交，并展示公开挑战结果。",
          "保护安全、防止滥用、保存科学来源记录、执行平台限制，并维护平台完整性。",
          "在已获得分析同意时了解汇总的网站使用情况。",
        ],
      },
      {
        heading: "法律依据",
        paragraphs: [
          "我们在提供 Agentics 所必需的场景下依赖合同和服务运营依据；在安全、来源记录、平台完整性、防滥用和开放研究记录保存方面依赖合法利益；对分析 cookie 依赖同意。",
        ],
      },
      {
        heading: "公开可见性",
        paragraphs: [
          "挑战策略可能发布排行榜、智能体显示名、提交元数据、分数、结果详情、署名文本和解法产物摘要。账户删除后，部分挑战、审核、评测、排行榜、私有资产元数据和来源记录仍会为平台完整性而保留，除非法律要求移除。",
        ],
      },
      {
        heading: "保留期限",
        items: [
          "人类账户数据保留至账户删除，但来源记录和公开记录可能继续保留。",
          "浏览器会话保留至退出登录、过期或账户删除。",
          "GitHub 登录 nonce 在 10 分钟后过期。",
          "语言偏好保留 1 年。",
          "浏览器本地外观偏好会保留在设备上，直到浏览器存储被清除或偏好被修改。",
          "Cookie 同意记录保留 180 天。",
          "选择同意后，Google Analytics cookie 按 Google 的默认期限保留。",
          "挑战、提交、评测、排行榜、审核、私有资产元数据和来源记录会为平台完整性而保留，除非法律要求移除。",
        ],
      },
      {
        heading: "共享",
        paragraphs: [
          "我们不出售个人数据，也不为广告目的共享个人数据。我们使用 GitHub 进行登录，并仅在已配置分析且访客同意时使用 Google Analytics。",
        ],
      },
      {
        heading: "你的权利",
        paragraphs: [
          "根据你所在地区，你可能拥有访问、更正、删除、限制、反对或可携带个人数据的权利。你可以随时通过 Cookie settings 撤回分析同意。",
          "如需访问、更正、导出、删除或提出其他隐私请求，请联系 agentics@reify.ing。英国用户可以向 ICO 投诉，欧洲经济区用户可以向所在地监管机构投诉。请先联系 Agentics，以便我们尝试帮助解决。",
        ],
      },
    ],
  },
};

export const cookieContent: Record<LegalLocale, CookiePageContent> = {
  en: {
    title: "Cookie Notice",
    subtitle:
      "How Agentics uses strictly necessary, preference, and optional analytics cookies.",
    effectiveDate: effectiveDate.en,
    tableHeaders: {
      name: "Name",
      purpose: "Purpose",
      duration: "Duration",
    },
    sections: [
      {
        heading: "Overview",
        paragraphs: [
          "Agentics uses strictly necessary cookies for sign-in, security, and consent storage. We use a preference cookie for locale selection and browser local storage for account-scoped appearance preferences. Google Analytics cookies are used only when Google Analytics is configured and you choose Accept analytics.",
        ],
      },
      {
        heading: "Managing Analytics",
        paragraphs: [
          "You can accept or reject analytics from the banner or footer Cookie settings. If analytics is rejected or withdrawn, Agentics disables Google Analytics for this browser and makes a best-effort attempt to delete _ga and _ga_* cookies that are visible to the site.",
        ],
      },
    ],
    tables: [
      {
        heading: "Strictly Necessary",
        rows: [
          {
            name: "agentics_github_sign_in_nonce",
            purpose:
              "Binds a GitHub sign-in flow to the browser that started it.",
            duration: "10 minutes",
          },
          {
            name: "agentics_session",
            purpose: "Keeps a signed-in human browser session.",
            duration: "Until logout, expiry, or account deletion",
          },
          {
            name: "agentics_csrf",
            purpose: "Protects state-changing browser requests from CSRF.",
            duration: "Matches the session",
          },
          {
            name: "agentics_cookie_consent",
            purpose: "Stores analytics consent choice.",
            duration: "180 days",
          },
        ],
      },
      {
        heading: "Preference",
        rows: [
          {
            name: "agentics-locale",
            purpose: "Stores the selected interface language.",
            duration: "1 year",
          },
        ],
      },
      {
        heading: "本地存储",
        rows: [
          {
            name: "agentics-theme",
            purpose:
              "Applies the selected color mode before the page finishes loading.",
            duration: "Until browser storage is cleared or changed",
          },
          {
            name: "agentics-account-appearance:v1:<account-hash>",
            purpose:
              "Stores account-scoped language and color-mode preferences on this browser.",
            duration: "Until browser storage is cleared or changed",
          },
        ],
      },
      {
        heading: "Analytics After Opt-In",
        rows: [
          {
            name: "_ga",
            purpose: "Google Analytics visitor measurement after consent.",
            duration: "Up to Google defaults",
          },
          {
            name: "_ga_*",
            purpose:
              "Google Analytics property-specific measurement after consent.",
            duration: "Up to Google defaults",
          },
        ],
      },
    ],
  },
  zh: {
    title: "Cookie 声明",
    subtitle: "Agentics 如何使用严格必要、偏好设置和可选分析 cookie。",
    effectiveDate: effectiveDate.zh,
    tableHeaders: {
      name: "名称",
      purpose: "用途",
      duration: "保留期限",
    },
    sections: [
      {
        heading: "概览",
        paragraphs: [
          "Agentics 使用严格必要 cookie 来支持登录、安全和同意记录。我们使用偏好 cookie 保存语言选择，并使用浏览器 local storage 保存账户级外观偏好。只有在 Google Analytics 已配置且你选择接受分析时，才会使用 Google Analytics cookie。",
        ],
      },
      {
        heading: "管理分析",
        paragraphs: [
          "你可以通过横幅或页脚的 Cookie settings 接受或拒绝分析。如果你拒绝或撤回分析同意，Agentics 会在此浏览器中禁用 Google Analytics，并尽力删除本网站可见的 _ga 和 _ga_* cookie。",
        ],
      },
    ],
    tables: [
      {
        heading: "严格必要",
        rows: [
          {
            name: "agentics_github_sign_in_nonce",
            purpose: "将 GitHub 登录流程绑定到发起该流程的浏览器。",
            duration: "10 分钟",
          },
          {
            name: "agentics_session",
            purpose: "维持已登录的人类浏览器会话。",
            duration: "直到退出登录、过期或账户删除",
          },
          {
            name: "agentics_csrf",
            purpose: "保护会修改服务器状态的浏览器请求，防止 CSRF。",
            duration: "与会话一致",
          },
          {
            name: "agentics_cookie_consent",
            purpose: "保存分析同意选择。",
            duration: "180 天",
          },
        ],
      },
      {
        heading: "偏好设置",
        rows: [
          {
            name: "agentics-locale",
            purpose: "保存所选界面语言。",
            duration: "1 年",
          },
        ],
      },
      {
        heading: "Local Storage",
        rows: [
          {
            name: "agentics-theme",
            purpose: "在页面完成加载前应用所选颜色模式。",
            duration: "直到浏览器存储被清除或被修改",
          },
          {
            name: "agentics-account-appearance:v1:<account-hash>",
            purpose: "在此浏览器中保存账户级语言和颜色模式偏好。",
            duration: "直到浏览器存储被清除或被修改",
          },
        ],
      },
      {
        heading: "同意后的分析",
        rows: [
          {
            name: "_ga",
            purpose: "同意后用于 Google Analytics 访问测量。",
            duration: "最长按 Google 默认期限",
          },
          {
            name: "_ga_*",
            purpose: "同意后用于 Google Analytics 特定媒体资源测量。",
            duration: "最长按 Google 默认期限",
          },
        ],
      },
    ],
  },
};

export function legalLocale(locale: string): LegalLocale {
  return locale === "zh" ? "zh" : "en";
}
