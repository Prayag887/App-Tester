import 'package:flutter/material.dart';

import '../../../shared/brand/app_tester_mark.dart';
import 'proxy_safety_view_model.dart';

class ProxySafetyScreen extends StatelessWidget {
  const ProxySafetyScreen({required this.viewModel, super.key});

  final ProxySafetyViewModel viewModel;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: viewModel,
        builder: (context, _) {
          final active = viewModel.status?.isVpnActive == true;
          final packageName = viewModel.status?.targetPackage;
          return Scaffold(
            body: SafeArea(
              child: Center(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.all(24),
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 520),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        const Row(children: [
                          AppTesterMark(size: 48),
                          SizedBox(width: 14),
                          Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text('App Tester',
                                    style: TextStyle(
                                        fontSize: 21,
                                        fontWeight: FontWeight.w700)),
                                Text('USB Companion',
                                    style: TextStyle(color: Color(0xff8ea6c9))),
                              ]),
                        ]),
                        const SizedBox(height: 36),
                        Container(
                          padding: const EdgeInsets.all(24),
                          decoration: BoxDecoration(
                            color: active
                                ? const Color(0xff103831)
                                : const Color(0xff0d1d35),
                            borderRadius: BorderRadius.circular(22),
                            border: Border.all(
                                color: active
                                    ? const Color(0xff246f5d)
                                    : const Color(0xff203651)),
                          ),
                          child: Column(children: [
                            Icon(
                                active
                                    ? Icons.usb_rounded
                                    : Icons.usb_off_rounded,
                                size: 72,
                                color: active
                                    ? const Color(0xff2be0a7)
                                    : const Color(0xff82baff)),
                            const SizedBox(height: 18),
                            Text(
                                active
                                    ? 'USB capture active'
                                    : 'Waiting for desktop',
                                style: Theme.of(context)
                                    .textTheme
                                    .headlineSmall
                                    ?.copyWith(fontWeight: FontWeight.w700)),
                            const SizedBox(height: 10),
                            Text(
                              active
                                  ? 'Capturing only ${packageName ?? 'selected app'}. Disconnecting USB stops interception and restores direct networking.'
                                  : 'Connect this phone with USB, then choose Open companion or Start capture in App Tester desktop.',
                              textAlign: TextAlign.center,
                              style: const TextStyle(
                                  color: Color(0xffaebfd8), height: 1.5),
                            ),
                          ]),
                        ),
                        if (viewModel.error != null) ...[
                          const SizedBox(height: 16),
                          Text(viewModel.error!,
                              style: const TextStyle(color: Color(0xffff9bac))),
                        ],
                        if (viewModel.status?.message case final message?) ...[
                          const SizedBox(height: 16),
                          Container(
                            padding: const EdgeInsets.all(14),
                            decoration: BoxDecoration(
                              color: const Color(0xff13243b),
                              borderRadius: BorderRadius.circular(14),
                            ),
                            child: Text(message,
                                textAlign: TextAlign.center,
                                style: const TextStyle(
                                    color: Color(0xffb8c9e3), height: 1.4)),
                          ),
                        ],
                        const SizedBox(height: 20),
                        FilledButton.icon(
                          onPressed: viewModel.isWorking ||
                                  active ||
                                  viewModel.status?.host == null ||
                                  packageName == null
                              ? null
                              : viewModel.connect,
                          icon: Icon(active
                              ? Icons.desktop_windows_rounded
                              : Icons.cable_rounded),
                          label: Text(active
                              ? 'Connected to desktop'
                              : 'Connect to desktop'),
                          style: FilledButton.styleFrom(
                              minimumSize: const Size.fromHeight(56)),
                        ),
                        const SizedBox(height: 12),
                        OutlinedButton.icon(
                          onPressed: viewModel.isWorking ||
                                  viewModel.status?.caAvailable != true
                              ? null
                              : viewModel.installCa,
                          icon: const Icon(Icons.verified_user_outlined),
                          label: const Text('Install HTTPS CA'),
                          style: OutlinedButton.styleFrom(
                              minimumSize: const Size.fromHeight(52)),
                        ),
                        const SizedBox(height: 12),
                        OutlinedButton.icon(
                          onPressed:
                              viewModel.isWorking ? null : viewModel.removeCa,
                          icon: const Icon(Icons.remove_moderator_outlined),
                          label: const Text('Remove HTTPS CA'),
                          style: OutlinedButton.styleFrom(
                              minimumSize: const Size.fromHeight(52)),
                        ),
                        const SizedBox(height: 12),
                        OutlinedButton.icon(
                          onPressed:
                              viewModel.isWorking ? null : viewModel.disconnect,
                          icon: const Icon(Icons.link_off_rounded),
                          label: Text(active
                              ? 'Stop interception'
                              : 'Ensure interception is stopped'),
                          style: OutlinedButton.styleFrom(
                              minimumSize: const Size.fromHeight(52)),
                        ),
                        const SizedBox(height: 12),
                        const Text(
                          'For HTTPS, install AppTester-HTTPS-CA.pem from Downloads. Remove HTTPS CA opens Android Trusted credentials, where Android requires you to confirm removal. Removing USB or tapping Stop interception always restores direct networking.',
                          textAlign: TextAlign.center,
                          style:
                              TextStyle(color: Color(0xff8ea6c9), height: 1.4),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      );
}
