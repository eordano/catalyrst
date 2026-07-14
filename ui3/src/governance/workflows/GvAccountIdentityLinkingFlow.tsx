import type { ReactNode } from "react";
import { useState } from "react";
import { GovernanceChromeMaybe } from "../frames/GovernanceChrome";
import { Close } from "../../atoms/icons";
import "./gvaccountidentitylinkingflow.css";

const CircledForum = ({ size = 64 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="32" cy="32" r="32" fill="#FCE9EC" />
    <path
      d="M32.5975 20.625C26.3033 20.625 21 25.627 21 31.802C21 32 21.0051 43.375 21.0051 43.375L32.5975 43.3648C38.8969 43.3648 44 38.1699 44 31.9949C44 25.8199 38.8969 20.625 32.5975 20.625ZM32.5 38.5C31.504 38.5 30.5542 38.2816 29.7071 37.8855L25.5435 38.9062L26.7192 35.0977C26.2161 34.1785 25.9286 33.1223 25.9286 32C25.9286 28.4098 28.8703 25.5 32.5 25.5C36.1297 25.5 39.0714 28.4098 39.0714 32C39.0714 35.5902 36.1297 38.5 32.5 38.5Z"
      fill="#FF2D55"
    />
  </svg>
);

const CircledDiscord = ({ size = 64 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="32" cy="32" r="32" fill="#E0DCF3" />
    <path
      d="M40.3091 24.8644C40.3025 24.8514 40.2915 24.8412 40.2781 24.8357C38.7293 24.1183 37.0949 23.6067 35.4158 23.3138C35.4006 23.3109 35.3848 23.313 35.3708 23.3197C35.3567 23.3264 35.3452 23.3374 35.3377 23.3511C35.1151 23.7589 34.9131 24.1778 34.7324 24.6062C32.9224 24.3288 31.0813 24.3288 29.2713 24.6062C29.0894 24.1767 28.8841 23.7577 28.6565 23.3511C28.6487 23.3377 28.637 23.3269 28.6231 23.3203C28.6091 23.3136 28.5935 23.3113 28.5783 23.3138C26.899 23.6061 25.2645 24.1177 23.716 24.8357C23.7027 24.8414 23.6915 24.8511 23.684 24.8634C20.5872 29.5326 19.7388 34.0869 20.155 38.5849C20.1562 38.5959 20.1595 38.6066 20.1649 38.6162C20.1702 38.6259 20.1774 38.6344 20.1861 38.6413C21.9893 39.9893 24.0063 41.0182 26.1508 41.6842C26.1659 41.6887 26.1821 41.6885 26.197 41.6836C26.212 41.6786 26.2251 41.6691 26.2346 41.6564C26.6953 41.0237 27.1034 40.3536 27.4549 39.6531C27.4597 39.6435 27.4625 39.633 27.463 39.6222C27.4635 39.6115 27.4617 39.6007 27.4578 39.5907C27.4539 39.5807 27.4479 39.5716 27.4403 39.564C27.4327 39.5565 27.4235 39.5507 27.4135 39.547C26.7699 39.2984 26.1468 38.9987 25.5501 38.6507C25.5393 38.6443 25.5302 38.6352 25.5236 38.6244C25.517 38.6136 25.5132 38.6013 25.5125 38.5886C25.5117 38.5759 25.5141 38.5633 25.5193 38.5517C25.5246 38.5402 25.5326 38.5301 25.5426 38.5225C25.6678 38.4278 25.7931 38.3292 25.9126 38.2297C25.9233 38.2209 25.9361 38.2152 25.9498 38.2134C25.9634 38.2115 25.9773 38.2136 25.9898 38.2192C29.8991 40.0205 34.1315 40.0205 37.9945 38.2192C38.0071 38.2132 38.0211 38.2109 38.035 38.2126C38.0488 38.2143 38.0619 38.2199 38.0727 38.2288C38.1923 38.3283 38.3175 38.4278 38.4437 38.5225C38.4537 38.5301 38.4618 38.5401 38.4671 38.5515C38.4725 38.563 38.4749 38.5757 38.4743 38.5884C38.4736 38.601 38.4699 38.6133 38.4634 38.6242C38.4569 38.6351 38.4479 38.6442 38.4371 38.6507C37.8417 39.0016 37.2181 39.3011 36.5728 39.5461C36.5628 39.5499 36.5537 39.5558 36.5461 39.5635C36.5385 39.5711 36.5326 39.5803 36.5288 39.5904C36.5249 39.6005 36.5233 39.6113 36.5238 39.6222C36.5244 39.633 36.5273 39.6435 36.5322 39.6531C36.8896 40.3497 37.2971 41.0189 37.7515 41.6553C37.7607 41.6684 37.7738 41.6782 37.7888 41.6833C37.8039 41.6885 37.8201 41.6887 37.8353 41.684C39.9837 41.0203 42.0042 39.9913 43.8097 38.6413C43.8185 38.6348 43.8258 38.6265 43.8312 38.6169C43.8365 38.6074 43.8398 38.5967 43.8408 38.5858C44.339 33.3857 43.0067 28.8687 40.3091 24.8644ZM28.0387 35.8461C26.8617 35.8461 25.8919 34.7556 25.8919 33.4164C25.8919 32.0771 26.8429 30.9865 28.0387 30.9865C29.2439 30.9865 30.2043 32.0866 30.1855 33.4163C30.1855 34.7556 29.2344 35.8461 28.0387 35.8461ZM35.976 35.8461C34.7991 35.8461 33.8293 34.7556 33.8293 33.4164C33.8293 32.0771 34.7803 30.9865 35.976 30.9865C37.1813 30.9865 38.1416 32.0866 38.1228 33.4163C38.1228 34.7556 37.1813 35.8461 35.976 35.8461Z"
      fill="#5D5FEF"
    />
  </svg>
);

const CircledPush = ({ size = 64 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="32" cy="32" r="32" fill="#FF5AFE" fillOpacity="0.16" />
    <circle cx="32" cy="32" r="14" fill="#EFEFF0" />
    <path
      d="M32 23.5c-3.9 0-7 3.1-7 7 0 2.2.9 4 2.4 5.3-.1 1.4-.7 2.9-1.6 3.9.7 0 1.8-.2 3-.9.7.2 1.5.4 2.4.4h.8c3.9 0 7-3.1 7-7s-3.1-7-7-7H32zm-1.6 9.5c-.8 0-1.4-.6-1.4-1.4s.6-1.4 1.4-1.4 1.4.6 1.4 1.4-.6 1.4-1.4 1.4zm3.2 0c-.8 0-1.4-.6-1.4-1.4s.6-1.4 1.4-1.4 1.4.6 1.4 1.4-.6 1.4-1.4 1.4z"
      fill="#C640CD"
    />
  </svg>
);

const Sign = () => (
  <svg width="56" height="56" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="32" cy="32" r="32" fill="#FCE9EC" />
    <path
      d="M27.8 26.25C27.8 25.524 28.3809 24.9375 29.1 24.9375C29.8191 24.9375 30.4 25.524 30.4 26.25V26.5699C30.4 27.7061 30.3025 28.8381 30.1116 29.9537L26.6828 30.9914C25.0334 31.4918 23.9041 33.0258 23.9041 34.7648V37.7139C23.9041 39.3545 25.2244 40.6875 26.8494 40.6875C27.9056 40.6875 28.8806 40.1174 29.4047 39.1904L29.9694 38.1938C31.0581 36.266 31.8584 34.1865 32.3419 32.0209L36.1769 30.8602L35.6691 32.3982C35.535 32.8002 35.6041 33.2391 35.8478 33.5795C36.0916 33.9199 36.4856 34.125 36.9041 34.125H42.1C42.8191 34.125 43.4 33.5385 43.4 32.8125C43.4 32.0865 42.8191 31.5 42.1 31.5H38.7037L39.435 29.2893C39.5894 28.8258 39.4716 28.3131 39.1344 27.9604C38.7972 27.6076 38.2934 27.4764 37.8262 27.6158L32.8537 29.1252C32.9512 28.2762 33 27.4271 33 26.5699V26.25C33 24.0762 31.2531 22.3125 29.1 22.3125C26.9469 22.3125 25.2 24.0762 25.2 26.25V27.5625C25.2 28.2885 25.7809 28.875 26.5 28.875C27.2191 28.875 27.8 28.2885 27.8 27.5625V26.25ZM27.4262 33.5098L29.4169 32.9068C28.9944 34.2932 28.4216 35.6303 27.7066 36.8936L27.1419 37.8902C27.0809 37.9969 26.9672 38.0666 26.8412 38.0666C26.6503 38.0666 26.4959 37.9107 26.4959 37.718V34.7648C26.4959 34.1865 26.8737 33.6738 27.4222 33.5057L27.4262 33.5098ZM20.975 36.0938C20.4347 36.0938 20 36.5326 20 37.0781C20 37.6236 20.4347 38.0625 20.975 38.0625H22.6122C22.6041 37.9477 22.6 37.8328 22.6 37.7139V36.0938H20.975ZM45.025 38.0625C45.5653 38.0625 46 37.6236 46 37.0781C46 36.5326 45.5653 36.0938 45.025 36.0938H32.4272C32.155 36.7623 31.8503 37.4186 31.5212 38.0625H45.025Z"
      fill="#FF2D55"
    />
  </svg>
);

const Copy = () => (
  <svg width="56" height="56" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="32" cy="32" r="32" fill="#FCE9EC" />
    <path
      d="M39.5 37H32C30.6211 37 29.5 35.8789 29.5 34.5V24.5C29.5 23.1211 30.6211 22 32 22H37.4727C37.9688 22 38.4453 22.1992 38.7969 22.5508L41.4492 25.2031C41.8008 25.5547 42 26.0312 42 26.5273V34.5C42 35.8789 40.8789 37 39.5 37ZM24.5 27H28.25V28.875H24.5C24.1562 28.875 23.875 29.1562 23.875 29.5V39.5C23.875 39.8438 24.1562 40.125 24.5 40.125H32C32.3438 40.125 32.625 39.8438 32.625 39.5V38.25H34.5V39.5C34.5 40.8789 33.3789 42 32 42H24.5C23.1211 42 22 40.8789 22 39.5V29.5C22 28.1211 23.1211 27 24.5 27Z"
      fill="#FF2D55"
    />
  </svg>
);

const Comment = () => (
  <svg width="56" height="56" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="32" cy="32" r="32" fill="#FCE9EC" />
    <path
      d="M28.5703 37.0352C27.9844 36.8242 27.332 36.9141 26.8281 37.2852C26.5078 37.5195 25.957 37.8633 25.2891 38.1719C25.5078 37.5977 25.6758 36.9492 25.7305 36.2422C25.7695 35.7383 25.6016 35.2383 25.2695 34.8555C24.3594 33.8281 23.875 32.625 23.875 31.375C23.875 28.2695 27.1289 25.125 32 25.125C36.8711 25.125 40.125 28.2695 40.125 31.375C40.125 34.4805 36.8711 37.625 32 37.625C30.7656 37.625 29.6055 37.4102 28.5703 37.0352ZM23.0273 38.5547C22.9648 38.6602 22.8984 38.7656 22.8281 38.8711L22.8164 38.8906C22.7539 38.9805 22.6914 39.0703 22.6289 39.1602C22.4922 39.3438 22.3438 39.5234 22.1875 39.6875C22.0078 39.8672 21.957 40.1328 22.0547 40.3672C22.1523 40.6016 22.3789 40.7539 22.6328 40.7539C22.832 40.7539 23.0312 40.7422 23.2305 40.7227L23.2578 40.7188C23.4297 40.6992 23.6016 40.6758 23.7734 40.6445C23.8047 40.6406 23.8359 40.6328 23.8672 40.625C24.5625 40.4883 25.2305 40.2539 25.8242 39.9961C26.7188 39.6055 27.4805 39.1406 27.9453 38.8008C29.1875 39.25 30.5625 39.5 32.0117 39.5C37.5352 39.5 42.0117 35.8633 42.0117 31.375C42.0117 26.8867 37.5234 23.25 32 23.25C26.4766 23.25 22 26.8867 22 31.375C22 33.1367 22.6914 34.7656 23.8633 36.0977C23.7891 37.0547 23.418 37.9062 23.0273 38.5547Z"
      fill="#FF2D55"
    />
  </svg>
);

const CheckCircle = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
    <circle cx="16" cy="16" r="16" fill="#44B600" />
    <path
      fill="#fff"
      d="M14.95 20.947a1.697 1.697 0 01-2.437 0l-3.998-3.998a1.697 1.697 0 010-2.436 1.697 1.697 0 012.436 0l2.811 2.749 6.747-6.747a1.697 1.697 0 012.436 0 1.697 1.697 0 010 2.436l-7.996 7.996z"
    />
  </svg>
);

const Lock = () => (
  <svg width="12" height="14" viewBox="0 0 12 14" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path
      d="M3.85714 3.9375V5.25H8.14286V3.9375C8.14286 2.72891 7.18393 1.75 6 1.75C4.81607 1.75 3.85714 2.72891 3.85714 3.9375ZM2.14286 5.25V3.9375C2.14286 1.76367 3.87054 0 6 0C8.12946 0 9.85714 1.76367 9.85714 3.9375V5.25H10.2857C11.2312 5.25 12 6.03477 12 7V12.25C12 13.2152 11.2312 14 10.2857 14H1.71429C0.76875 14 0 13.2152 0 12.25V7C0 6.03477 0.76875 5.25 1.71429 5.25H2.14286Z"
      fill="rgba(22, 20, 26, 0.2)"
    />
  </svg>
);

const LinkSucceeded = () => (
  <svg width="72" height="40" viewBox="0 0 72 40" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="4" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="20" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="52" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="68" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="36" cy="19" r="16" fill="#44B600" />
    <path d="M28.5 19.5L33 24L43 14" stroke="white" strokeWidth="4" />
  </svg>
);

const LinkFailed = () => (
  <svg width="72" height="40" viewBox="0 0 72 40" fill="none" xmlns="http://www.w3.org/2000/svg">
    <circle cx="4" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="20" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="52" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="68" cy="19" r="4" fill="#D9D9D9" />
    <circle cx="36" cy="19" r="16" fill="#D80027" />
    <path d="M31 24L41 14" stroke="white" strokeWidth="4" />
    <path d="M41 24L31 14" stroke="white" strokeWidth="4" />
  </svg>
);

const ValidatedProfile = () => (
  <svg width="9" height="12" viewBox="0 0 9 12" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path
      d="M1.5 0C0.672656 0 0 0.672656 0 1.5V10.5C0 11.3273 0.672656 12 1.5 12H7.5C8.32734 12 9 11.3273 9 10.5V1.5C9 0.672656 8.32734 0 7.5 0H1.5ZM3.75 7.5H5.25C6.28594 7.5 7.125 8.33906 7.125 9.375C7.125 9.58125 6.95625 9.75 6.75 9.75H2.25C2.04375 9.75 1.875 9.58125 1.875 9.375C1.875 8.33906 2.71406 7.5 3.75 7.5ZM3 5.25C3 4.85218 3.15804 4.47064 3.43934 4.18934C3.72064 3.90804 4.10218 3.75 4.5 3.75C4.89782 3.75 5.27936 3.90804 5.56066 4.18934C5.84196 4.47064 6 4.85218 6 5.25C6 5.64782 5.84196 6.02936 5.56066 6.31066C5.27936 6.59196 4.89782 6.75 4.5 6.75C4.10218 6.75 3.72064 6.59196 3.43934 6.31066C3.15804 6.02936 3 5.64782 3 5.25ZM3.375 1H5.625C5.83125 1 6 1.29375 6 1.5C6 1.70625 5.83125 2 5.625 2H3.375C3.16875 2 3 1.70625 3 1.5C3 1.29375 3.16875 1 3.375 1Z"
      fill="url(#gvail_vp)"
    />
    <defs>
      <linearGradient id="gvail_vp" x1="8.5" y1="0" x2="0" y2="12" gradientUnits="userSpaceOnUse">
        <stop stopColor="#C640CD" />
        <stop offset="1" stopColor="#314ADE" />
      </linearGradient>
    </defs>
  </svg>
);

const ForumBlue = ({ size = 90 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 96 97" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path
      d="M48.4071 0.8125C22.1357 0.8125 0 21.7822 0 47.6697C0 48.5 0.0214286 96.1875 0.0214286 96.1875L48.4071 96.1449C74.7 96.1449 96 74.3662 96 48.4787C96 22.5912 74.7 0.8125 48.4071 0.8125ZM48 75.75C43.8429 75.75 39.8786 74.8346 36.3429 73.174L18.9643 77.4531L23.8714 61.4863C21.7714 57.633 20.5714 53.2049 20.5714 48.5C20.5714 33.4486 32.85 21.25 48 21.25C63.15 21.25 75.4286 33.4486 75.4286 48.5C75.4286 63.5514 63.15 75.75 48 75.75Z"
      fill="#1768E9"
    />
  </svg>
);

const Discord90 = () => (
  <svg width="90" height="90" viewBox="0 0 71 55" fill="none" xmlns="http://www.w3.org/2000/svg">
    <path
      d="M60.1045 4.8978C55.5792 2.8214 50.7265 1.2916 45.6527 0.41542C45.5603 0.39851 45.468 0.440769 45.4204 0.525289C44.7963 1.6353 44.105 3.0834 43.6209 4.2216C38.1637 3.4046 32.7345 3.4046 27.3892 4.2216C26.905 3.0581 26.1886 1.6353 25.5617 0.525289C25.5141 0.443589 25.4218 0.40133 25.3294 0.41542C20.2584 1.2888 15.4057 2.8186 10.8776 4.8978C10.8384 4.9147 10.8048 4.9429 10.7825 4.9795C1.57795 18.7309 -0.943561 32.1443 0.293408 45.3914C0.299005 45.4562 0.335386 45.5182 0.385761 45.5576C6.45866 50.0174 12.3413 52.7249 18.1147 54.5195C18.2071 54.5477 18.305 54.5139 18.3638 54.4378C19.7295 52.5728 20.9469 50.6063 21.9907 48.5383C22.0523 48.4172 21.9935 48.2735 21.8676 48.2256C19.9366 47.4931 18.0979 46.6 16.3292 45.5858C16.1893 45.5041 16.1781 45.304 16.3068 45.2082C16.679 44.9293 17.0513 44.6391 17.4067 44.3461C17.471 44.2926 17.5606 44.2813 17.6362 44.3151C29.2558 49.6202 41.8354 49.6202 53.3179 44.3151C53.3935 44.2785 53.4831 44.2898 53.5502 44.3433C53.9057 44.6363 54.2779 44.9293 54.6529 45.2082C54.7816 45.304 54.7732 45.5041 54.6333 45.5858C52.8646 46.6197 51.0259 47.4931 49.0921 48.2228C48.9662 48.2707 48.9102 48.4172 48.9718 48.5383C50.038 50.6034 51.2554 52.5699 52.5959 54.435C52.6519 54.5139 52.7526 54.5477 52.845 54.5195C58.6464 52.7249 64.529 50.0174 70.6019 45.5576C70.6551 45.5182 70.6887 45.459 70.6943 45.3942C72.1747 30.0791 68.2147 16.7757 60.1968 4.9823C60.1772 4.9429 60.1437 4.9147 60.1045 4.8978ZM23.7259 37.3253C20.2276 37.3253 17.3451 34.1136 17.3451 30.1693C17.3451 26.225 20.1717 23.0133 23.7259 23.0133C27.308 23.0133 30.1626 26.2532 30.1066 30.1693C30.1066 34.1136 27.28 37.3253 23.7259 37.3253ZM47.3178 37.3253C43.8196 37.3253 40.9371 34.1136 40.9371 30.1693C40.9371 26.225 43.7636 23.0133 47.3178 23.0133C50.9 23.0133 53.7545 26.2532 53.6986 30.1693C53.6986 34.1136 50.9 37.3253 47.3178 37.3253Z"
      fill="var(--blue-900)"
    />
  </svg>
);

const SETUP_TITLE = "Link your Decentraland profile to external services";

type Provider = "forum" | "discord" | "push";

type ProviderInfo = {
  title: string;
  description: string;
  helper?: string;
  icon: ReactNode;
};

const PROVIDERS: Record<Provider, ProviderInfo> = {
  forum: {
    title: "Decentraland Forum",
    description: "Publish comments on proposals using your Decentraland profile.",
    icon: <CircledForum />,
  },
  discord: {
    title: "Discord",
    description: "Receive real-time notifications customized to your activity and never miss any important events.",
    icon: <CircledDiscord />,
  },
  push: {
    title: "Push Protocol Notifications",
    description: "Receive notifications in your wallet or on any platform using this native web3 messaging protocol.",
    helper: "TRIGGERS MESSAGE SIGN REQUEST",
    icon: <CircledPush />,
  },
};

const FLOW_STEPS: Record<"forum" | "discord", { title: string; confirm: string }> = {
  forum: { title: "Decentraland Forum Account", confirm: "Confirm link" },
  discord: { title: "Discord Account", confirm: "Confirm link" },
};

type StepDef = {
  icon: ReactNode;
  title: string;
  description: string;
  action: string;
  helper: string;
};

const STEP_DEFS: StepDef[] = [
  {
    icon: <Sign />,
    title: "1. Sign message",
    description: "Use your web3 wallet to securely sign a message to start the linking process.",
    action: "Sign",
    helper: "Your web3 wallet will trigger a signing request",
  },
  {
    icon: <Copy />,
    title: "2. Copy to clipboard",
    description: "You'll be using the same content as the signature for the next step.",
    action: "Copy",
    helper: "Signature required",
  },
  {
    icon: <Comment />,
    title: "3. Post on Forum thread",
    description: "Paste the content onto the comment box and publish to complete the linking.",
    action: "Open",
    helper: "https://forum.decentraland.org/t/...",
  },
];
const DISCORD_STEP3 = {
  title: "3. Post on Discord channel",
  description: "Paste the content onto the designated channel to complete the linking.",
};
const FLOW_HELPERS = ["First, sign the message.", "Then, copy the signed message.", "Finally, comment the signed message on the forum thread."];

type PostInfo = {
  success_text: string;
  success_body: string;
  success_subtext: string;
  success_button: string;
  icon: ReactNode;
};

const POST: Record<Provider, PostInfo> = {
  forum: {
    success_text: "**Decentraland and Forum accounts linked successfully** \u{1F389}",
    success_body: "Now the comments you left on Proposals will link to your Decentraland profile. Happy governance!",
    success_subtext: "Plus, you'll get a fancy icon next to your username",
    success_button: "Take me to my Profile",
    icon: <ForumBlue />,
  },
  discord: {
    success_text: "**Discord and Governance accounts linked successfully** \u{1F389}",
    success_body: "You'll start getting notified via Discord DM when there's new activity in proposals relevant to you.",
    success_subtext: " ",
    success_button: "Take me to my Profile",
    icon: <Discord90 />,
  },
  push: {
    success_text: "**Push Notifications enabled successfully!**",
    success_body: "",
    success_subtext: "Now you will receive Governance-related notifications in your Push account.",
    success_button: "Connect other accounts",
    icon: <CircledPush size={90} />,
  },
};
const POST_ERROR = {
  text: "**Governance and Forum accounts could not be linked**",
  body: "Something happened. It must have been aliens or a malevolent AI but we can't be sure.",
  subtext: "Maybe just have a go at it one more time?",
  button: "Retry",
};

function ActionCard({
  icon,
  title,
  description,
  helper,
  action,
  isVerified,
  isDisabled,
  onClick,
}: {
  icon: ReactNode;
  title: ReactNode;
  description: ReactNode;
  helper?: ReactNode;
  action?: ReactNode;
  isVerified?: boolean;
  isDisabled?: boolean;
  onClick?: () => void;
}) {
  const actionable = !isDisabled && !isVerified && !!onClick;
  const cls =
    "gvail__card-row" +
    (isDisabled ? " is-disabled" : "") +
    (!actionable ? " is-static" : "") +
    (action ? " has-action" : "");
  return (
    <div className={cls} onClick={actionable ? onClick : undefined} role={actionable ? "button" : undefined}>
      <div className="gvail__card-icon">{icon}</div>
      <div className="gvail__card-body">
        <h3>
          {title}
          {isVerified && <span className="gvail__label gvail__label--verified">Verified</span>}
        </h3>
        <p>{description}</p>
        {!!helper && <p className="gvail__helper">{helper}</p>}
      </div>
      {!!action && <div className="gvail__card-action">{action}</div>}
    </div>
  );
}

function ChooseAccountView({ withLinked }: { withLinked?: boolean }) {
  return (
    <>
      <div className="gvail__header">
        <div>{SETUP_TITLE}</div>
      </div>
      <div className="gvail__content">
        {withLinked && (
          <UnlinkAccountCard account="forum" username="dao_delegate" verificationDate="2 months ago" />
        )}
        <ActionCard {...PROVIDERS.forum} onClick={() => {}} />
        <ActionCard {...PROVIDERS.discord} onClick={() => {}} />
        <ActionCard {...PROVIDERS.push} onClick={() => {}} />
      </div>
    </>
  );
}

function ConnectionFlowView({
  provider,
  currentStep = 1,
  showTimer,
}: {
  provider: "forum" | "discord";
  currentStep?: number;
  showTimer?: boolean;
}) {
  const meta = FLOW_STEPS[provider];
  const steps = STEP_DEFS.map((s, i) => {
    if (provider === "discord" && i === 2) return { ...s, ...DISCORD_STEP3 };
    return s;
  });
  return (
    <>
      <div className="gvail__header">
        <div>{meta.title}</div>
        {showTimer && <div className="gvail__timer">Time sensitive task. 4:38 left to complete</div>}
      </div>
      <div className="gvail__content">
        {steps.map((step, i) => {
          const stepIdx = i + 1;
          const isDisabled = stepIdx > currentStep;
          const isCompleted = stepIdx < currentStep;
          let action;
          if (isCompleted) action = <CheckCircle size={24} />;
          else if (isDisabled) action = <Lock />;
          else action = <button className="gvail__btn gvail__btn--basic">{step.action}</button>;
          return (
            <ActionCard
              key={i}
              icon={step.icon}
              title={step.title}
              description={step.description}
              helper={step.helper}
              action={action}
              isDisabled={isDisabled}
            />
          );
        })}
        <div className="gvail__helper-container">
          <button className="gvail__btn gvail__btn--primary" disabled>
            {meta.confirm}
          </button>
          <div className="gvail__helper-text">{FLOW_HELPERS[currentStep - 1]}</div>
        </div>
      </div>
    </>
  );
}

function PostConnectionView({ provider, isValidated = true }: { provider: Provider; isValidated?: boolean }) {
  const data = POST[provider];
  return (
    <div className="gvail__content">
      <div className="gvail__post-icons">
        <div className="gvail__avatar-xxl" />
        {isValidated ? <LinkSucceeded /> : <LinkFailed />}
        {data.icon}
      </div>
      <div className="gvail__post-text">
        <p>
          <strong>{isValidated ? strip(data.success_text) : strip(POST_ERROR.text)}</strong>
        </p>
        {(isValidated ? data.success_body : POST_ERROR.body) && (
          <p>{isValidated ? data.success_body : POST_ERROR.body}</p>
        )}
        <div className="gvail__post-subtext">
          <p>{isValidated ? data.success_subtext : POST_ERROR.subtext}</p>
          {isValidated && provider === "forum" && <ValidatedProfile />}
        </div>
      </div>
      <div className="gvail__post-action">
        <button className="gvail__btn gvail__btn--primary">
          {isValidated ? data.success_button : POST_ERROR.button}
        </button>
      </div>
    </div>
  );
}

function PushSubscribingView({ state = "subscribing" }: { state?: string }) {
  return (
    <div className="gvail__content">
      <div className="gvail__push-state">
        {state === "subscribing" ? (
          <>
            <div className="gvail__spinner" />
            <h4>Subscribing to Push notifications&#x2026;</h4>
            <p>Confirm the signature request in your wallet to enable Governance notifications via Push Protocol.</p>
          </>
        ) : (
          <>
            <LinkFailed />
            <h4>Couldn't subscribe</h4>
            <p>Something went wrong while subscribing. Please try again.</p>
            <button className="gvail__btn gvail__btn--primary">Retry</button>
          </>
        )}
      </div>
    </div>
  );
}

function UnlinkAccountCard({
  account,
  username,
  verificationDate,
  onUnlink,
}: {
  account: string;
  username: string;
  verificationDate: string;
  onUnlink?: () => void;
}) {
  const detail = username ? `@${username} \u{2022} Linked ${verificationDate}` : `Linked ${verificationDate}`;
  return (
    <div className="gvail__unlink">
      <div className="gvail__unlink-data">
        <div className="gvail__unlink-title">
          {account}
          <CheckCircle size={16} />
        </div>
        <div className="gvail__unlink-details">{detail}</div>
      </div>
      <button className="gvail__unlink-action" onClick={onUnlink}>
        Unlink
      </button>
    </div>
  );
}

function UnlinkConfirmView({ onClose }: { onClose: () => void }) {
  return (
    <div className="gvail__confirm-backdrop" onClick={onClose}>
      <div className="gvail__confirm" onClick={(e) => e.stopPropagation()}>
        <h2>Unlink Confirmation</h2>
        <p>Are you sure you want to unlink this account?</p>
        <div className="gvail__confirm-actions">
          <button className="gvail__btn gvail__btn--primary" onClick={onClose}>
            Unlink
          </button>
          <button className="gvail__btn gvail__btn--basic" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

function strip(s: string) {
  return s.replace(/\*\*/g, "");
}

export default function GvAccountIdentityLinkingFlow({ chrome = true, initial = "choose" }: { chrome?: boolean; initial?: string }) {
  const [view, setView] = useState(initial);

  let body;
  switch (view) {
    case "choose":
      body = <ChooseAccountView />;
      break;
    case "unlink-row":
      body = <ChooseAccountView withLinked />;
      break;
    case "forum":
      body = <ConnectionFlowView provider="forum" currentStep={2} showTimer />;
      break;
    case "discord":
      body = <ConnectionFlowView provider="discord" currentStep={1} />;
      break;
    case "push":
      body = <PushSubscribingView state="subscribing" />;
      break;
    case "post-success":
      body = <PostConnectionView provider="forum" isValidated />;
      break;
    case "post-error":
      body = <PostConnectionView provider="forum" isValidated={false} />;
      break;
    case "unlink-confirm":
      body = <ChooseAccountView withLinked />;
      break;
    default:
      body = <ChooseAccountView />;
  }

  return (
    <GovernanceChromeMaybe chrome={chrome} active={null}>
      <div className="gvail">
        <div className="gvail__card">
          <button className="gvail__close" aria-label="Close">
            <Close size={24} />
          </button>
          {body}
        </div>
      </div>

      {view === "unlink-confirm" && <UnlinkConfirmView onClose={() => setView("unlink-row")} />}
    </GovernanceChromeMaybe>
  );
}
