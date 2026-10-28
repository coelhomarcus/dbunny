// The DBunny rabbit, as the designer drew it (dbunny-symbol.svg), cropped to the drawing. The body takes the
// ink of the app icon in each theme (dark on light, warm white on dark); the orange accent is the brand's.
const BODY = "M 336 138 C 310 138 294 158 294 190 C 294 254 330 312 378 359 L 502 473 C 516 486 522 502 522 520 C 522 541 512 552 490 552 L 466 552 C 357 552 268 639 268 742 C 268 807 305 854 356 854 L 538 854 C 538 837 525 824 508 824 L 474 824 C 419 824 376 781 376 726 L 376 638 Q 376 626 388 626 L 488 626 C 555 626 608 679 608 746 L 608 774 C 608 808 597 835 576 854 L 692 854 C 721 854 740 836 740 808 C 740 754 710 704 672 663 Q 660 650 678 647 L 703 643 C 736 638 758 617 758 588 C 758 568 736 530 715 503 C 691 472 665 459 627 445 C 598 434 580 418 567 391 L 533 318 C 485 218 401 138 336 138 Z M 366 240 C 409 248 454 292 482 344 C 501 379 513 416 518 446 C 453 394 388 326 362 252 Q 358 240 366 240 Z M 634 522 C 663 522 687 542 694 571 C 665 571 641 551 634 522 Z";
const EAR = "M 538 185 C 520 185 510 203 510 226 C 510 240 514 250 522 263 C 558 323 598 389 642 438 C 647 344 622 254 577 207 C 562 192 549 185 538 185 Z";
const ACCENT = "M 424 676 L 486 676 C 532 676 568 712 568 758 L 568 774 Q 568 784 558 784 L 478 784 C 441 784 412 755 412 718 L 412 688 Q 412 676 424 676 Z";

/** The DBunny rabbit without its tile, sized like an icon (`className` sets the size). */
export default function Logo({ className }: { className?: string }) {
  return (
    <svg viewBox="133 116 760 760" className={className} aria-hidden="true">
      <g className="fill-[#171920] dark:fill-[#F8F3EB]">
        <path fillRule="evenodd" d={BODY} />
        <path d={EAR} />
      </g>
      <path fill="#EA5A1E" d={ACCENT} />
    </svg>
  );
}
