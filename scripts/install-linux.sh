#!/usr/bin/env bash
set -eu

# get latest zip
repo="trdthg/t-autotest"

echo ">>> checking latest tag..."
tag=$(wget -q -O- https://api.github.com/repos/${repo}/releases/latest | jq -r '.tag_name')
echo "<<< latest tag: $tag"

# save dir
echo ">>> prepare dir"
folder="$HOME/.autotest"
if [ -d "$folder" ]; then
    rm -r "$folder"
fi
mkdir -p $folder
echo "<<< prepare success"

echo ">>> downloading zip..."
cd $folder
zip_name=autotest-linux.tar.gz
wget https://github.com/trdthg/t-autotest/releases/download/$tag/$zip_name -O autotest.tar.gz
echo "<<< download success"

echo ">>> extracting..."
tar -zxvf autotest.tar.gz
echo "<<< extrace success"

cd --

# set env
echo ">>> setting env(.bashrc)..."
echo "export PATH=$PATH:$folder" >> ~/.bashrc
export PATH="$PATH:$folder"
echo "<<< done!, you can try with 'autotest -v'"
